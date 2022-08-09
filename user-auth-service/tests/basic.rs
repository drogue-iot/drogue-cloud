mod common;

use actix_web::{web, App};
use drogue_client::user::v1::authz::{AuthorizationRequest, Permission};
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_test_common::{client, db};
use drogue_cloud_user_auth_service::{endpoints, service, WebData};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_auth_deny_non_existing() {
    test_auth!(AuthorizationRequest{
        application: "appX".into(),
        permission: Permission::Owner,
        user_id: Some("userX".into()),
        roles: vec![],
    } => json!("deny"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_deny_non_existing_but_admin() {
    test_auth!(AuthorizationRequest{
        application: "appX".into(),
        permission: Permission::Owner,
        user_id: Some("userX".into()),
        roles: vec!["drogue-user".into(), "drogue-admin".into()],
    } => json!("deny"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_allow_owner() {
    test_auth!(AuthorizationRequest{
        application: "app1".into(),
        permission: Permission::Owner,
        user_id: Some("user1".into()),
        roles: vec![],
    } => json!("allow"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_deny_non_owner() {
    test_auth!(AuthorizationRequest{
        application: "app1".into(),
        permission: Permission::Owner,
        user_id: Some("user2".into()),
        roles: vec![],
    } => json!("deny"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_allow_non_owner_but_admin() {
    test_auth!(AuthorizationRequest{
        application: "app1".into(),
        permission: Permission::Owner,
        user_id: Some("user2".into()),
        roles: vec!["drogue-user".into(), "drogue-admin".into()],
    } => json!("allow"));
}
