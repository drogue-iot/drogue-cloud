use drogue_cloud_database_common::{
    error::ServiceError,
    models::{Constraints, Resource},
};
use uuid::Uuid;

/// check if an expected UUID is equal to the actual one.
///
/// Returns `None` if the UUIDs don't match, otherwise `Some` containing the UUID.
fn is_ok_and_equal(expected: &str, actual: Uuid) -> Option<Uuid> {
    if expected.is_empty() {
        return Some(actual);
    }

    match uuid::Uuid::parse_str(expected) {
        Ok(expected) if expected == actual => Some(expected),
        _ => None,
    }
}

/// Check if the expected UID and version match the provided current state.
///
/// The function will also return a set of `Constraints`, which maybe be used further on for
/// optimistic locking.
pub fn check_versions<S1, S2>(
    expected_uid: S1,
    expected_resource_version: S2,
    current: &dyn Resource,
) -> Result<Constraints, ServiceError>
where
    S1: AsRef<str>,
    S2: AsRef<str>,
{
    let expected_uid = expected_uid.as_ref();
    let expected_resource_version = expected_resource_version.as_ref();

    // check the uid

    let uid = if !expected_uid.is_empty() {
        if let Some(expected_uid) = is_ok_and_equal(expected_uid, current.uid()) {
            expected_uid
        } else {
            return Err(ServiceError::Conflict(format!(
                "Update request for non-existent ID - current: {}, requested: {}",
                current.uid(),
                expected_uid
            )));
        }
    } else {
        current.uid()
    };

    // check the resource version

    let resource_version = if let Some(expected_resource_version) =
        is_ok_and_equal(expected_resource_version, current.resource_version())
    {
        expected_resource_version
    } else {
        return Err(ServiceError::Conflict(format!(
            "Update request for modified object - current: {}, requested: {}",
            current.resource_version(),
            expected_resource_version
        )));
    };

    // return result

    Ok(Constraints {
        uid,
        resource_version,
    })
}
