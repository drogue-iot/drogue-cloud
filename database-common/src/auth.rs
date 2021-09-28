//! Common authn/authz logic

use crate::{error::ServiceError, models::app::MemberEntry};
use drogue_cloud_service_api::admin::Role;
use drogue_cloud_service_api::auth::user::{
    authz::{Outcome, Permission},
    UserInformation,
};
use indexmap::map::IndexMap;
use std::fmt::Debug;

/// A resource that can be checked.
pub trait Resource: Debug {
    fn owner(&self) -> Option<&str>;
    fn members(&self) -> &IndexMap<String, MemberEntry>;
}

/// Authorize an operation.
///
/// Currently, this is a rather simple approach. If the resource has an owner, the owners must match
/// to grant access.
///
/// NOTE: This logic must be aligned with [`super::models::sql::SelectBuilder::auth()`]
pub fn authorize(
    resource: &dyn Resource,
    identity: &UserInformation,
    permission: Permission,
) -> Outcome {
    log::debug!(
        "authorizing - resource: {:?}, identity: {:?}, permission: {:?}",
        resource,
        identity,
        permission
    );

    // if we are "admin", grant access
    if identity.is_admin() {
        log::debug!("Granting access as user is admin");
        return Outcome::Allow;
    }

    // check the owner
    match (resource.owner(), identity.user_id()) {
        // If there is no owner -> allow access
        (None, _) => Outcome::Allow,
        // If there is an owner and an authenticated user and both match -> allow access
        (Some(owner), Some(user)) if owner == user => Outcome::Allow,
        // We must be owner, but are not -> deny
        _ if permission == Permission::Owner => Outcome::Deny,
        // Check the member list
        (Some(_), user) => {
            // If we don't have a user, look for the anonymous mapping
            let user = user.unwrap_or_default();
            // If there is a member in the list which matches the user ...
            if let Some(member) = resource.members().get(user) {
                match permission {
                    // this should already be covered be the rule above
                    Permission::Owner => Outcome::Deny,
                    Permission::Admin => match member.role {
                        Role::Admin => Outcome::Allow,
                        _ => Outcome::Deny,
                    },
                    Permission::Write => match member.role {
                        Role::Admin | Role::Manager => Outcome::Allow,
                        _ => Outcome::Deny,
                    },
                    Permission::Read => Outcome::Allow,
                }
            } else {
                Outcome::Deny
            }
        }
    }
}

/// Ensure an operation is authorized.
///
/// This will call [`authorize`] and transform the result into a [`Result`]. It will return
/// [`ServiceError::NotAuthorized`] in case of [`Outcome::Deny`].
pub fn ensure(
    resource: &dyn Resource,
    identity: &UserInformation,
    permission: Permission,
) -> Result<(), ServiceError> {
    ensure_with(resource, identity, permission, || {
        ServiceError::NotAuthorized
    })
}

/// Ensure an operation is authorized.
///
/// This will call [`authorize`] and transform the result into a [`Result`]. It will return the
/// return value of the function in case of [`Outcome::Deny`].
pub fn ensure_with<F>(
    resource: &dyn Resource,
    identity: &UserInformation,
    permission: Permission,
    f: F,
) -> Result<(), ServiceError>
where
    F: FnOnce() -> ServiceError,
{
    match authorize(resource, identity, permission) {
        Outcome::Allow => Ok(()),
        Outcome::Deny => Err(f()),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use drogue_cloud_service_api::auth::user::UserDetails;

    #[derive(Debug)]
    struct MockResource {
        owner: String,
        members: IndexMap<String, MemberEntry>,
    }

    impl Resource for MockResource {
        fn owner(&self) -> Option<&str> {
            Some(&self.owner)
        }

        fn members(&self) -> &IndexMap<String, MemberEntry> {
            &self.members
        }
    }

    macro_rules! resource {
        ($owner:literal, [$( $id:literal => $role:expr),*]) => {
            {
                #[allow(unused_mut)]
                let mut members = IndexMap::new();
                $(
                    members.insert($id.into(), MemberEntry { role: $role });
                )*
                MockResource {
                    owner: $owner.into(),
                    members,
                }
            }

        };
    }

    macro_rules! test_auth {
        ($user:expr, $resource:expr, [$( $perm:expr => $outcome:expr ),*]) => {
            {
                let user = $user;
                let resource = $resource;
                $(assert_eq!(authorize(&user, &resource, $perm), $outcome, "Expected outcome '{:?}' for permission '{:?}'", $outcome, $perm);)*
            }
        };
    }

    fn user(id: &str, roles: &[&str]) -> UserInformation {
        UserInformation::Authenticated(UserDetails {
            user_id: id.into(),
            roles: roles.iter().map(ToString::to_string).collect(),
        })
    }

    #[test]
    fn test_auth_owner() {
        test_auth!(
            resource!("foo", []),
            user("foo", &[]),
            [
                Permission::Owner => Outcome::Allow,
                Permission::Admin => Outcome::Allow,
                Permission::Write => Outcome::Allow,
                Permission::Read => Outcome::Allow
            ]
        )
    }

    #[test]
    fn test_auth_sys_admin() {
        test_auth!(
            resource!("foo", []),
            user("bar", &["drogue-admin"]),
            [
                Permission::Owner => Outcome::Allow,
                Permission::Admin => Outcome::Allow,
                Permission::Write => Outcome::Allow,
                Permission::Read => Outcome::Allow
            ]
        )
    }

    #[test]
    fn test_auth_resource_admin() {
        test_auth!(
            resource!("foo", ["bar" => Role::Admin]),
            user("bar", &[]),
            [
                Permission::Owner => Outcome::Deny,
                Permission::Admin => Outcome::Allow,
                Permission::Write => Outcome::Allow,
                Permission::Read => Outcome::Allow
            ]
        )
    }

    #[test]
    fn test_auth_resource_manager() {
        test_auth!(
            resource!("foo", ["bar" => Role::Manager]),
            user("bar", &[]),
            [
                Permission::Owner => Outcome::Deny,
                Permission::Admin => Outcome::Deny,
                Permission::Write => Outcome::Allow,
                Permission::Read => Outcome::Allow
            ]
        )
    }

    #[test]
    fn test_auth_resource_reader() {
        test_auth!(
            resource!("foo", ["bar" => Role::Reader]),
            user("bar", &[]),
            [
                Permission::Owner => Outcome::Deny,
                Permission::Admin => Outcome::Deny,
                Permission::Write => Outcome::Deny,
                Permission::Read => Outcome::Allow
            ]
        )
    }

    #[test]
    fn test_auth_anon() {
        test_auth!(
            resource!("foo", ["" => Role::Reader]),
            UserInformation::Anonymous,
            [
                Permission::Owner => Outcome::Deny,
                Permission::Admin => Outcome::Deny,
                Permission::Write => Outcome::Deny,
                Permission::Read => Outcome::Allow
            ]
        )
    }

    #[test]
    fn test_reject_anon() {
        test_auth!(
            resource!("foo", ["bar" => Role::Reader]),
            UserInformation::Anonymous,
            [
                Permission::Owner => Outcome::Deny,
                Permission::Admin => Outcome::Deny,
                Permission::Write => Outcome::Deny,
                Permission::Read => Outcome::Deny
            ]
        )
    }
}
