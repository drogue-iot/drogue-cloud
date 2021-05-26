mod common;

use actix_web::{test, web, App};
use drogue_cloud_service_api::auth::user::authz::{AuthorizationRequest, Permission};
use drogue_cloud_test_common::{client, db};
use drogue_cloud_user_auth_service::{endpoints, service, WebData};
use serde_json::json;
use serial_test::serial;

macro_rules! test_member {
    ($app:literal, $user:literal, [$($roles:literal),*], $permission:expr => $outcome:literal) => {
        test_auth!(AuthorizationRequest{
            application: $app.into(),
            permission: $permission,
            user_id: $user.into(),
            roles: Vec::from([$($roles,)*]),
        } => json!($outcome));
    };
}

#[actix_rt::test]
#[serial]
async fn test_auth_member_admin() {
    test_member!("app-member1", "bar-admin", [], Permission::Owner => "deny");
    test_member!("app-member1", "bar-admin", [], Permission::Admin => "allow");
    test_member!("app-member1", "bar-admin", [], Permission::Write => "allow");
    test_member!("app-member1", "bar-admin", [], Permission::Read => "allow");
}

#[actix_rt::test]
#[serial]
async fn test_auth_member_manager() {
    test_member!("app-member1", "bar-manager", [], Permission::Owner => "deny");
    test_member!("app-member1", "bar-manager", [], Permission::Admin => "deny");
    test_member!("app-member1", "bar-manager", [], Permission::Write => "allow");
    test_member!("app-member1", "bar-manager", [], Permission::Read => "allow");
}

#[actix_rt::test]
#[serial]
async fn test_auth_member_reader() {
    test_member!("app-member1", "bar-reader", [], Permission::Owner => "deny");
    test_member!("app-member1", "bar-reader", [], Permission::Admin => "deny");
    test_member!("app-member1", "bar-reader", [], Permission::Write => "deny");
    test_member!("app-member1", "bar-reader", [], Permission::Read => "allow");
}

#[actix_rt::test]
#[serial]
async fn test_auth_member_anon() {
    test_member!("app-member1", "", [], Permission::Owner => "deny");
    test_member!("app-member1", "", [], Permission::Admin => "deny");
    test_member!("app-member1", "", [], Permission::Write => "deny");
    test_member!("app-member1", "", [], Permission::Read => "allow");
}
