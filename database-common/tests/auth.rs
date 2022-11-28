use drogue_cloud_database_common::auth::{authorize, Resource};
use drogue_cloud_service_api::auth::user::UserDetails;
use drogue_cloud_service_common::auth::UserInformation;

use drogue_client::admin::v1::{MemberEntry, Role};
use drogue_client::user::v1::authz::{
    ApplicationPermission, DevicePermission, Outcome, Permission,
};

use indexmap::IndexMap;

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
                    members.insert($id.into(), MemberEntry { roles: $role });
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
            Permission::App(ApplicationPermission::Create) => Outcome::Allow,
            Permission::App(ApplicationPermission::Delete) => Outcome::Allow,
            Permission::App(ApplicationPermission::Write) => Outcome::Allow,
            Permission::App(ApplicationPermission::Read) => Outcome::Allow,
            Permission::App(ApplicationPermission::Transfer) => Outcome::Allow,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Allow,
            Permission::App(ApplicationPermission::Command) => Outcome::Allow,
            Permission::App(ApplicationPermission::Members) => Outcome::Allow,
            Permission::Device(DevicePermission::Create) => Outcome::Allow,
            Permission::Device(DevicePermission::Delete) => Outcome::Allow,
            Permission::Device(DevicePermission::Write) => Outcome::Allow,
            Permission::Device(DevicePermission::Read) => Outcome::Allow
        ]
    )
}

#[test]
fn test_auth_sys_admin() {
    test_auth!(
        resource!("foo", []),
        user("bar", &["drogue-admin"]),
        [
            Permission::App(ApplicationPermission::Create) => Outcome::Allow,
            Permission::App(ApplicationPermission::Delete) => Outcome::Allow,
            Permission::App(ApplicationPermission::Write) => Outcome::Allow,
            Permission::App(ApplicationPermission::Read) => Outcome::Allow,
            Permission::App(ApplicationPermission::Transfer) => Outcome::Allow,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Allow,
            Permission::App(ApplicationPermission::Command) => Outcome::Allow,
            Permission::App(ApplicationPermission::Members) => Outcome::Allow,
            Permission::Device(DevicePermission::Create) => Outcome::Allow,
            Permission::Device(DevicePermission::Delete) => Outcome::Allow,
            Permission::Device(DevicePermission::Write) => Outcome::Allow,
            Permission::Device(DevicePermission::Read) => Outcome::Allow
        ]
    )
}

#[test]
fn test_auth_resource_admin() {
    test_auth!(
        resource!("foo", ["bar" => vec![Role::Admin]]),
        user("bar", &[]),
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Create) => Outcome::Allow,
            Permission::App(ApplicationPermission::Delete) => Outcome::Allow,
            Permission::App(ApplicationPermission::Write) => Outcome::Allow,
            Permission::App(ApplicationPermission::Read) => Outcome::Allow,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Allow,
            Permission::App(ApplicationPermission::Command) => Outcome::Allow,
            Permission::App(ApplicationPermission::Members) => Outcome::Allow,
            Permission::Device(DevicePermission::Create) => Outcome::Allow,
            Permission::Device(DevicePermission::Delete) => Outcome::Allow,
            Permission::Device(DevicePermission::Write) => Outcome::Allow,
            Permission::Device(DevicePermission::Read) => Outcome::Allow
        ]
    )
}

#[test]
fn test_auth_resource_manager() {
    test_auth!(
        resource!("foo", ["bar" =>  vec![Role::Manager]]),
        user("bar", &[]),
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Allow,
            Permission::App(ApplicationPermission::Read) => Outcome::Allow,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Deny,
            Permission::App(ApplicationPermission::Command) => Outcome::Deny,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Allow,
            Permission::Device(DevicePermission::Delete) => Outcome::Allow,
            Permission::Device(DevicePermission::Write) => Outcome::Allow,
            Permission::Device(DevicePermission::Read) => Outcome::Allow
        ]
    )
}

#[test]
fn test_auth_resource_reader_subscriber() {
    test_auth!(
        resource!("foo", ["bar" =>  vec![Role::Reader, Role::Subscriber]]),
        user("bar", &[]),
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Deny,
            Permission::App(ApplicationPermission::Read) => Outcome::Allow,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Allow,
            Permission::App(ApplicationPermission::Command) => Outcome::Deny,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Deny,
            Permission::Device(DevicePermission::Delete) => Outcome::Deny,
            Permission::Device(DevicePermission::Write) => Outcome::Deny,
            Permission::Device(DevicePermission::Read) => Outcome::Allow
        ]
    )
}

#[test]
fn test_auth_resource_reader() {
    test_auth!(
        resource!("foo", ["bar" => vec![Role::Reader]]),
        user("bar", &[]),
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Deny,
            Permission::App(ApplicationPermission::Read) => Outcome::Allow,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Deny,
            Permission::App(ApplicationPermission::Command) => Outcome::Deny,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Deny,
            Permission::Device(DevicePermission::Delete) => Outcome::Deny,
            Permission::Device(DevicePermission::Write) => Outcome::Deny,
            Permission::Device(DevicePermission::Read) => Outcome::Allow
        ]
    )
}

#[test]
fn test_auth_resource_subscriber() {
    test_auth!(
        resource!("foo", ["bar" => vec![Role::Subscriber]]),
        user("bar", &[]),
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Deny,
            Permission::App(ApplicationPermission::Read) => Outcome::Deny,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Allow,
            Permission::App(ApplicationPermission::Command) => Outcome::Deny,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Deny,
            Permission::Device(DevicePermission::Delete) => Outcome::Deny,
            Permission::Device(DevicePermission::Write) => Outcome::Deny,
            Permission::Device(DevicePermission::Read) => Outcome::Deny
        ]
    )
}

#[test]
fn test_auth_resource_publisher() {
    test_auth!(
        resource!("foo", ["bar" => vec![Role::Publisher]]),
        user("bar", &[]),
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Deny,
            Permission::App(ApplicationPermission::Read) => Outcome::Deny,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Deny,
            Permission::App(ApplicationPermission::Command) => Outcome::Allow,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Deny,
            Permission::Device(DevicePermission::Delete) => Outcome::Deny,
            Permission::Device(DevicePermission::Write) => Outcome::Deny,
            Permission::Device(DevicePermission::Read) => Outcome::Deny
        ]
    )
}

#[test]
fn test_auth_resource_publisher_subscriber() {
    test_auth!(
        resource!("foo", ["bar" => vec![Role::Publisher, Role::Subscriber]]),
        user("bar", &[]),
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Deny,
            Permission::App(ApplicationPermission::Read) => Outcome::Deny,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Allow,
            Permission::App(ApplicationPermission::Command) => Outcome::Allow,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Deny,
            Permission::Device(DevicePermission::Delete) => Outcome::Deny,
            Permission::Device(DevicePermission::Write) => Outcome::Deny,
            Permission::Device(DevicePermission::Read) => Outcome::Deny
        ]
    )
}

#[test]
fn test_auth_anon() {
    test_auth!(
        resource!("foo", ["" => vec![Role::Subscriber, Role::Reader]]),
        UserInformation::Anonymous,
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Create) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Deny,
            Permission::App(ApplicationPermission::Read) => Outcome::Allow,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Allow,
            Permission::App(ApplicationPermission::Command) => Outcome::Deny,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Deny,
            Permission::Device(DevicePermission::Delete) => Outcome::Deny,
            Permission::Device(DevicePermission::Write) => Outcome::Deny,
            Permission::Device(DevicePermission::Read) => Outcome::Allow
        ]
    )
}

#[test]
fn test_reject_anon() {
    test_auth!(
        resource!("foo", ["bar" => vec![Role::Reader]]),
        UserInformation::Anonymous,
        [
            Permission::App(ApplicationPermission::Transfer) => Outcome::Deny,
            Permission::App(ApplicationPermission::Create) => Outcome::Deny,
            Permission::App(ApplicationPermission::Delete) => Outcome::Deny,
            Permission::App(ApplicationPermission::Write) => Outcome::Deny,
            Permission::App(ApplicationPermission::Read) => Outcome::Deny,
            Permission::App(ApplicationPermission::Subscribe) => Outcome::Deny,
            Permission::App(ApplicationPermission::Command) => Outcome::Deny,
            Permission::App(ApplicationPermission::Members) => Outcome::Deny,
            Permission::Device(DevicePermission::Create) => Outcome::Deny,
            Permission::Device(DevicePermission::Delete) => Outcome::Deny,
            Permission::Device(DevicePermission::Write) => Outcome::Deny,
            Permission::Device(DevicePermission::Read) => Outcome::Deny
        ]
    )
}
