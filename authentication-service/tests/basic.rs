use actix_web::{test, web, App};
use drogue_cloud_authentication_service::{endpoints, service, WebData};
use drogue_cloud_service_api::auth::{AuthenticationRequest, Credential};
use drogue_cloud_test_common::{client, db};
use log::LevelFilter;
use serde_json::json;
use serial_test::serial;

fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

macro_rules! test {
   ($v:ident => $($code:block)*) => {{
        init();

        let cli = client();
        let db = db(&cli, |pg| service::AuthenticationServiceConfig{
            pg
        })?;

        let data = WebData {
            service: service::PostgresAuthenticationService::new(db.config.clone()).unwrap(),
        };

        let mut $v =
            test::init_service(drogue_cloud_authentication_service::app!(data, 16 * 1024)).await;

        $($code)*

        Ok(())
    }};
}

macro_rules! test_auth {
    ($rep:expr => $res:expr) => {
        test!(app => {
            let resp = test::TestRequest::post().uri("/api/v1/auth").set_json(&$rep).send_request(&mut app).await;
            let is_success = resp.status().is_success();
            let result: serde_json::Value = test::read_body_json(resp).await;

            let outcome = $res;

            assert_eq!(result, json!({"outcome": outcome}));
            assert!(is_success);
        })
    };
}

#[actix_rt::test]
#[serial]
async fn test_health() -> anyhow::Result<()> {
    test!(app => {
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp: serde_json::Value = test::read_response_json(&mut app, req).await;

        assert_eq!(resp, json!({"success": true}));
    })
}

/// Authorize a device using a password.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
        tenant: "tenant1".into(),
        device: "device1".into(),
        credential: Credential::Password("foo".into())
    } => json!({"pass":{
        "tenant": {"id": "tenant1", "data": {}},
        "device": {"tenant_id": "tenant1", "id": "device1", "data": {}}}
    }))
}

/// Authorize a device using a username/password combination for a password-only credential
/// that has a username matching the device ID.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password_with_device_username() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
        tenant: "tenant1".into(),
        device: "device1".into(),
        credential: Credential::UsernamePassword{username: "device1".into(), password: "foo".into()}
    } => json!({"pass":{
        "tenant": {"id": "tenant1", "data": {}},
        "device": {"tenant_id": "tenant1", "id": "device1", "data": {}}}
    }))
}

/// Authorize a device using a username/password combination for a password-only credential
/// that has a username matching the device ID.
#[actix_rt::test]
#[serial]
async fn test_auth_fails_password_with_non_matching_device_username() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
        tenant: "tenant1".into(),
        device: "device1".into(),
        credential: Credential::UsernamePassword{username: "device2".into(), password: "foo".into()}
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_wrong_password() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "device1".into(),
            credential: Credential::Password("foo1".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_tenant() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant2".into(),
            device: "device1".into(),
            credential: Credential::Password("foo".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_device() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "device2".into(),
            credential: Credential::Password("foo".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_passes_username_password() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "device3".into(),
            credential: Credential::UsernamePassword{username: "foo".into(), password: "bar".into()}
    } => json!({"pass":{
        "tenant": {"id": "tenant1", "data": {}},
        "device": {"tenant_id": "tenant1", "id": "device3", "data": {}}}
    }))
}

#[actix_rt::test]
#[serial]
async fn test_auth_passes_username_password_by_alias() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "12:34:56".into(),
            credential: Credential::UsernamePassword{username: "foo".into(), password: "bar".into()}
    } => json!({"pass":{
        "tenant": {"id": "tenant1", "data": {}},
        "device": {"tenant_id": "tenant1", "id": "device3", "data": {}}}
    }))
}

/// The password only variant must fail, as the username is not the device id.
#[actix_rt::test]
#[serial]
async fn test_auth_fails_password_only() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "device3".into(),
            credential: Credential::Password("bar".into())
    } => json!("fail"))
}

/// The password only variant must success, as the username is equal to the device id.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password_only() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "device3".into(),
            credential: Credential::Password("baz".into())
    }  => json!({"pass":{
        "tenant": {"id": "tenant1", "data": {}},
        "device": {"tenant_id": "tenant1", "id": "device3", "data": {}}}
    }))
}
