mod common;

use actix_web::{web, App};
use drogue_cloud_authentication_service::{endpoints, service, WebData};
use drogue_cloud_service_api::auth::device::authn::{AuthenticationRequest, Credential};
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_test_common::{client, db};
use rstest::rstest;
use serde_json::{json, Value};
use serial_test::serial;

fn device1_json() -> Value {
    json!({"pass":{
        "application": {
            "metadata": {
                "name": "app3",
                "uid": "4cf9607e-c7ad-11eb-8d69-d45d6455d2cc",
                "creationTimestamp": "2021-01-01T00:00:00Z",
                "resourceVersion": "547531d4-c7ad-11eb-abee-d45d6455d2cc",
                "generation": 0,
            },
        },
        "device": {
            "metadata": {
                "application": "app3",
                "name": "device1",
                "uid": "4e185ea6-7c26-11eb-a319-d45d6455d211",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        }
    }})
}

fn device2_json() -> Value {
    json!({"pass":{
        "application": {
            "metadata": {
                "name": "app3",
                "uid": "4cf9607e-c7ad-11eb-8d69-d45d6455d2cc",
                "creationTimestamp": "2021-01-01T00:00:00Z",
                "resourceVersion": "547531d4-c7ad-11eb-abee-d45d6455d2cc",
                "generation": 0,
            },
        },
        "device": {
            "metadata": {
                "application": "app3",
                "name": "device2",
                "uid": "8bcfeb78-c7ae-11eb-9535-d45d6455d2cc",
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
                "name": "app3",
                "uid": "4cf9607e-c7ad-11eb-8d69-d45d6455d2cc",
                "creationTimestamp": "2021-01-01T00:00:00Z",
                "resourceVersion": "547531d4-c7ad-11eb-abee-d45d6455d2cc",
                "generation": 0,
            },
        },
        "device": {
            "metadata": {
                "application": "app3",
                "name": "device3",
                "uid": "91023af6-c7ae-11eb-9902-d45d6455d2cc",
                "creationTimestamp": "2020-01-01T00:00:00Z",
                "resourceVersion": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
                "generation": 0,
            },
        }
    }})
}

/// Test different passwords with different stored types.
#[rstest]
#[case("device1", "foo", device1_json())]
#[case("device2", "foo", json!("fail"))]
#[case("device3", "foo", json!("fail"))]
#[case("device1", "bar", json!("fail"))]
#[case("device2", "bar", device2_json())]
#[case("device3", "bar", json!("fail"))]
#[case("device1", "baz", json!("fail"))]
#[case("device2", "baz", json!("fail"))]
#[case("device3", "baz", device3_json())]
#[actix_rt::test]
#[serial]
async fn test_auth_password_with_hashes(
    #[case] device: &str,
    #[case] password: &str,
    #[case] outcome: Value,
) {
    test_auth!(AuthenticationRequest{
        application: "app3".into(),
        device: device.to_string(),
        credential: Credential::Password(password.to_string()),
        r#as: None,
    } => outcome);
}
