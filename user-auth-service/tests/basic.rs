mod common;

use actix_web::{test, web, App};
use drogue_cloud_service_api::auth::authz::AuthorizationRequest;
use drogue_cloud_test_common::{client, db};
use drogue_cloud_user_auth_service::{endpoints, service, WebData};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_auth_deny_non_existing() -> anyhow::Result<()> {
    test_auth!(AuthorizationRequest{
        application: "appX".into(),
        user_id: "userX".into(),
    } => json!("deny"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_allow_owner() -> anyhow::Result<()> {
    test_auth!(AuthorizationRequest{
        application: "app1".into(),
        user_id: "user1".into(),
    } => json!("allow"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_deny_non_owner() -> anyhow::Result<()> {
    test_auth!(AuthorizationRequest{
        application: "app1".into(),
        user_id: "user2".into(),
    } => json!("deny"))
}
