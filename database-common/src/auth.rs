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
    log::warn!(
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
        match permission {
            Permission::App(permission) => {
                match permission {
                    // this should already be covered be the rule above
                    ApplicationPermission::Transfer => Outcome::Deny,
                    // short of transferring the app, the admin can do anything
                    _ if member.roles.contains(&Role::Admin) => Outcome::Allow,
                    // Read is allowed for Reader, Writer and Manager
                    ApplicationPermission::Read
                        if member.roles.contains(&Role::Reader)
                            || member.roles.contains(&Role::Manager) =>
                    {
                        Outcome::Allow
                    }
                    // Write is allowed for Admin and Manager
                    ApplicationPermission::Write if member.roles.contains(&Role::Manager) => {
                        Outcome::Allow
                    }

                    ApplicationPermission::Command if member.roles.contains(&Role::Publisher) => {
                        Outcome::Allow
                    }
                    ApplicationPermission::Subscribe
                        if member.roles.contains(&Role::Subscriber) =>
                    {
                        Outcome::Allow
                    }
                    _ => Outcome::Deny,
                }
            }
            Permission::Device(permission) => {
                // When it comes to devices the admin can do anything
                if member.roles.contains(&Role::Admin) {
                    return Outcome::Allow;
                }
                match permission {
                    DevicePermission::Read
                        if member.roles.contains(&Role::Reader)
                            || member.roles.contains(&Role::Manager) =>
                    {
                        Outcome::Allow
                    }
                    DevicePermission::Write
                    | DevicePermission::Create
                    | DevicePermission::Delete
                        if member.roles.contains(&Role::Manager) =>
                    {
                        Outcome::Allow
                    }
                    _ => Outcome::Deny,
                }
            }
            // TODO: implement permission check for access tokens operations
            Permission::Token(permission) => todo!(),
        }
    } else {
        Outcome::Deny
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
