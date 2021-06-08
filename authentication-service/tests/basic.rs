mod common;

use actix_web::{test, web, App};
use drogue_cloud_authentication_service::{endpoints, service, WebData};
use drogue_cloud_service_api::auth::device::authn::{AuthenticationRequest, Credential};
use drogue_cloud_test_common::{client, db};
use serde_json::{json, Value};
use serial_test::serial;

fn device1_json() -> Value {
    json!({"pass":{
        "application": {
            "metadata": {
                "name": "app1",
                "uid": "4e185ea6-7c26-11eb-a319-d45d6455d210",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        },
        "device": {
            "metadata": {
                "application": "app1",
                "name": "device1",
                "uid": "4e185ea6-7c26-11eb-a319-d45d6455d211",
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
                "uid": "4e185ea6-7c26-11eb-a319-d45d6455d210",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        },
        "device": {
            "metadata": {
                "application": "app1",
                "name": "device3",
                "uid": "4e185ea6-7c26-11eb-a319-d45d6455d212",
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
async fn test_auth_passes_password() {
    test_auth!(AuthenticationRequest{
        application: "app1".into(),
        device: "device1".into(),
        credential: Credential::Password("foo".into()),
        r#as: None,
    } => device1_json());
}

/// Authorize a device using a username/password combination for a password-only credential
/// that has a username matching the device ID.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password_with_device_username() {
    test_auth!(AuthenticationRequest{
        application: "app1".into(),
        device: "device1".into(),
        credential: Credential::UsernamePassword{username: "device1".into(), password: "foo".into()},
        r#as: None,
    } => device1_json());
}

/// Authorize a device using a username/password combination for a password-only credential
/// that has a username matching the device ID.
#[actix_rt::test]
#[serial]
async fn test_auth_fails_password_with_non_matching_device_username() {
    test_auth!(AuthenticationRequest{
        application: "app1".into(),
        device: "device1".into(),
        credential: Credential::UsernamePassword{username: "device2".into(), password: "foo".into()},
        r#as: None,
    } => json!("fail"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_wrong_password() {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device1".into(),
            credential: Credential::Password("foo1".into()),
            r#as: None,
    } => json!("fail"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_tenant() {
    test_auth!(AuthenticationRequest{
            application: "app2".into(),
            device: "device1".into(),
            credential: Credential::Password("foo".into()),
            r#as: None,
    } => json!("fail"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_device() {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device2".into(),
            credential: Credential::Password("foo".into()),
            r#as: None,
    } => json!("fail"));
}

#[actix_rt::test]
#[serial]
async fn test_auth_passes_username_password() {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device3".into(),
            credential: Credential::UsernamePassword{username: "foo".into(), password: "bar".into()},
            r#as: None,
    } => device3_json());
}

#[actix_rt::test]
#[serial]
async fn test_auth_passes_username_password_by_alias() {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "12:34:56".into(),
            credential: Credential::UsernamePassword{username: "foo".into(), password: "bar".into()},
            r#as: None,
    } => device3_json());
}

/// The password only variant must fail, as the username is not the device id.
#[actix_rt::test]
#[serial]
async fn test_auth_fails_password_only() {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device3".into(),
            credential: Credential::Password("bar".into()),
            r#as: None,
    } => json!("fail"));
}

/// The password only variant must success, as the username is equal to the device id.
#[actix_rt::test]
#[serial]
async fn test_auth_passes_password_only() {
    test_auth!(AuthenticationRequest{
            application: "app1".into(),
            device: "device3".into(),
            credential: Credential::Password("baz".into()),
            r#as: None,
    }  => device3_json());
}
