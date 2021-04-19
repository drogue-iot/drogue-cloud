//! Common authn/authz logic

use crate::error::ServiceError;
use drogue_cloud_service_api::auth::authz::Outcome;
use drogue_cloud_service_common::auth::Identity;

/// A resource that can be checked.
pub trait Resource {
    fn owner(&self) -> Option<&str>;
}

/// Authorize an operation.
///
/// Currently, this is a rather simple approach. If the resource has an owner, the owners must match
/// to grant access.
pub fn authorize(resource: &dyn Resource, identity: &dyn Identity) -> Outcome {
    // if we are "admin", grant access
    if identity.is_admin() {
        return Outcome::Allow;
    }

    match (resource.owner(), identity.user_id()) {
        // If there is no owner -> allow access
        (None, _) => Outcome::Allow,
        // If there is an owner, and an authenticated user and both match -> allow access
        (Some(owner), Some(user)) if owner == user => Outcome::Allow,
        // Otherwise -> deny access
        _ => Outcome::Deny,
    }
}

/// Ensure an operation is authorized.
///
/// This will call [`authorize`] and transform the result into a [`Result`]. It will return
/// [`ServiceError::NotAuthorized`] in case of [`Outcome::Deny`].
pub fn ensure(resource: &dyn Resource, identity: &dyn Identity) -> Result<(), ServiceError> {
    ensure_with(resource, identity, || ServiceError::NotAuthorized)
}

/// Ensure an operation is authorized.
///
/// This will call [`authorize`] and transform the result into a [`Result`]. It will return the
/// return value of the function in case of [`Outcome::Deny`].
pub fn ensure_with<F>(
    resource: &dyn Resource,
    identity: &dyn Identity,
    f: F,
) -> Result<(), ServiceError>
where
    F: FnOnce() -> ServiceError,
{
    match authorize(resource, identity) {
        Outcome::Allow => Ok(()),
        Outcome::Deny => Err(f()),
    }
}
