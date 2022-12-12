//! Common authn/authz logic

use crate::error::ServiceError;
use drogue_client::admin::v1::{MemberEntry, Role};
use drogue_client::user::v1::authz::{
    ApplicationPermission, DevicePermission, Outcome, Permission,
};
use drogue_cloud_service_api::auth::user::{IsAdmin, UserInformation};
use indexmap::map::IndexMap;
use std::fmt::Debug;

/// A resource that can be checked.
pub trait Resource: Debug {
    fn owner(&self) -> Option<&str>;
    fn members(&self) -> &IndexMap<String, MemberEntry>;
    fn name(&self) -> &String;
}

/// Authorize an operation.
///
/// If the UserInformation contains scopes, they are checked first
/// Then the members for an application are checked against the current user
///
/// NOTE: This logic must be aligned with [`super::models::sql::SelectBuilder::auth()`]
pub fn authorize(
    resource: &dyn Resource,
    identity: &UserInformation,
    permission: Permission,
) -> Outcome {
    log::warn!(
        "authorizing - resource: {:?}, identity: {:?}, permission: {:?}",
        resource,
        identity,
        permission
    );

    // If there are some limiting claims provided, we need to check them first.
    if let Some(claims) = identity.token_claims() {
        // the access token claims app creation permission
        if permission == Permission::App(ApplicationPermission::Create) && !claims.create {
            return Outcome::Deny;
        }

        if let Some(claimed_roles) = claims.applications.get(resource.name()) {
            // first check against the claimed roles
            match permission_table(permission, &claimed_roles.0) {
                // if the claimed roles pass, we carry on to  check against the actual user's role
                // to make sure the token isn't trying to escalate permissions
                Outcome::Allow => {}
                Outcome::Deny => return Outcome::Deny,
            }
        } else {
            // If the application is not even in the claims, don't bother checking anything
            return Outcome::Deny;
        }
    }

    // if we are "admin", grant access
    if identity.is_admin() {
        log::debug!("Granting access as user is admin");
        return Outcome::Allow;
    }

    // check the owner
    match (resource.owner(), identity.user_id()) {
        // If there is no owner -> allow access
        (None, _) => return Outcome::Allow,
        // If there is an owner and an authenticated user and both match -> allow access
        // as the owner have all permissions on a resource
        (Some(owner), Some(user)) if owner == user => return Outcome::Allow,
        // The current user is not the owner, carry on..
        _ => {}
    }

    // Check the member list
    // If we don't have a user, look for the anonymous mapping
    let user = identity.user_id().unwrap_or_default();
    // If there is a member in the list which matches the user ...
    if let Some(member) = resource.members().get(user) {
        permission_table(permission, &member.roles.0)
    } else {
        Outcome::Deny
    }
}

fn permission_table(permission: Permission, roles: &[Role]) -> Outcome {
    match permission {
        Permission::App(permission) => {
            match permission {
                // these should already by the owner rule
                ApplicationPermission::Transfer | ApplicationPermission::Delete => Outcome::Deny,
                // short of transferring the app, the admin can do anything
                _ if roles.contains(&Role::Admin) => Outcome::Allow,
                // Read is allowed for Reader, Writer and Manager
                ApplicationPermission::Read
                    if roles.contains(&Role::Reader) || roles.contains(&Role::Manager) =>
                {
                    Outcome::Allow
                }
                // Write is allowed for Admin and Manager
                ApplicationPermission::Write if roles.contains(&Role::Manager) => Outcome::Allow,

                ApplicationPermission::Command if roles.contains(&Role::Publisher) => {
                    Outcome::Allow
                }
                ApplicationPermission::Subscribe if roles.contains(&Role::Subscriber) => {
                    Outcome::Allow
                }
                _ => Outcome::Deny,
            }
        }
        Permission::Device(permission) => {
            // When it comes to devices the admin can do anything
            if roles.contains(&Role::Admin) {
                return Outcome::Allow;
            }
            match permission {
                DevicePermission::Read
                    if roles.contains(&Role::Reader) || roles.contains(&Role::Manager) =>
                {
                    Outcome::Allow
                }
                DevicePermission::Write | DevicePermission::Create | DevicePermission::Delete
                    if roles.contains(&Role::Manager) =>
                {
                    Outcome::Allow
                }
                _ => Outcome::Deny,
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
