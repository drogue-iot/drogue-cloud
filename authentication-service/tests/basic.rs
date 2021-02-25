mod common;

use actix_web::{test, web, App};
use drogue_cloud_authentication_service::{endpoints, service, WebData};
use drogue_cloud_service_api::auth::{AuthenticationRequest, Credential};
use drogue_cloud_test_common::{client, db};
use serde_json::{json, Value};
use serial_test::serial;

fn device1_json() -> Value {
    json!({"pass":{
        "application": {
            "metadata": {
                "name": "app1",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        },
        "device": {
            "metadata": {
                "application": "app1",
                "name": "device1",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        }
    }})
}

fn device3_json() -> Value {
    json!({"pass":{
        "application": {
            "metadata": {
                "name": "app1",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        },
        "device": {
            "metadata": {
                "application": "app1",
                "name": "device3",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        }
    }})
}

/// Authorize a device using a password.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
        application: "app1".into(),
        device: "device1".into(),
        credential: Credential::Password("foo".into())
    } => device1_json())
}

/// Authorize a device using a username/password combination for a password-only credential
/// that has a username matching the device ID.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password_with_device_username() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
        application: "app1".into(),
        device: "device1".into(),
        credential: Credential::UsernamePassword{username: "device1".into(), password: "foo".into()}
    } => device1_json())
}

/// Authorize a device using a username/password combination for a password-only credential
/// that has a username matching the device ID.
#[actix_rt::test]
#[serial]
async fn test_auth_fails_password_with_non_matching_device_username() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
        application: "app1".into(),
        device: "device1".into(),
        credential: Credential::UsernamePassword{username: "device2".into(), password: "foo".into()}
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_wrong_password() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device1".into(),
            credential: Credential::Password("foo1".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_tenant() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            application: "app2".into(),
            device: "device1".into(),
            credential: Credential::Password("foo".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_device() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device2".into(),
            credential: Credential::Password("foo".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_passes_username_password() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device3".into(),
            credential: Credential::UsernamePassword{username: "foo".into(), password: "bar".into()}
    } => device3_json())
}

#[actix_rt::test]
#[serial]
async fn test_auth_passes_username_password_by_alias() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "12:34:56".into(),
            credential: Credential::UsernamePassword{username: "foo".into(), password: "bar".into()}
    } => device3_json())
}

/// The password only variant must fail, as the username is not the device id.
#[actix_rt::test]
#[serial]
async fn test_auth_fails_password_only() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device3".into(),
            credential: Credential::Password("bar".into())
    } => json!("fail"))
}

/// The password only variant must success, as the username is equal to the device id.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password_only() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device3".into(),
            credential: Credential::Password("baz".into())
    }  => device3_json())
}
