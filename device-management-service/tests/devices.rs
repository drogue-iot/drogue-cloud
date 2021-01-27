mod common;

use crate::common::init;
use actix_cors::Cors;
use actix_web::{http::StatusCode, middleware::Condition, test, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self, PostgresManagementService},
    WebData,
};
use drogue_cloud_service_common::openid::AuthenticatorError;
use drogue_cloud_test_common::{client, db};
use http::{header, HeaderValue};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_create_device() -> anyhow::Result<()> {
    test!(app => {
        let resp = test::TestRequest::post().uri("/api/v1/tenants").set_json(&json!({
            "tenant_id": "tenant1",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/v1/tenants/tenant1")));

        let resp = test::TestRequest::post().uri("/api/v1/tenants/tenant1/devices").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/v1/tenants/tenant1/devices/device1")));
    })
}

#[actix_rt::test]
#[serial]
async fn test_create_device_no_tenant() -> anyhow::Result<()> {
    test!(app => {
        let resp = test::TestRequest::post().uri("/api/v1/tenants/tenant1/devices").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({"error": "ReferenceNotFound", "message": "Referenced a non-existing entity"}));
    })
}

/// Try some cases of "bad input data"
#[actix_rt::test]
#[serial]
async fn test_create_device_bad_request() -> anyhow::Result<()> {
    test!(app => {
        let resp = test::TestRequest::post().uri("/api/v1/tenants").set_json(&json!({
            "tenant_id": "tenant1",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = test::TestRequest::post().uri("/api/v1/tenants/tenant1/devices").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "",
            "password": "foo",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    })
}

#[actix_rt::test]
#[serial]
async fn test_create_duplicate_device() -> anyhow::Result<()> {
    test!(app => {
        let resp = test::TestRequest::post().uri("/api/v1/tenants").set_json(&json!({
            "tenant_id": "tenant1",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = test::TestRequest::post().uri("/api/v1/tenants/tenant1/devices").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = test::TestRequest::post().uri("/api/v1/tenants/tenant1/devices").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CONFLICT);
    })
}

#[actix_rt::test]
#[serial]
async fn test_crud_device() -> anyhow::Result<()> {
    test!(app => {

        // read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create tenant first
        let resp = test::TestRequest::post().uri("/api/v1/tenants").set_json(&json!({
            "tenant_id": "tenant1",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // read, must still not exist
        let resp = test::TestRequest::get().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create device
        let resp = test::TestRequest::post().uri("/api/v1/tenants/tenant1/devices").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // read, must exist now
        let resp = test::TestRequest::get().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({"tenant_id": "tenant1", "id": "device1", "data": {"credentials": [
            {"pass": "foo"}
        ]}}));

        // update device
        let resp = test::TestRequest::put().uri("/api/v1/tenants/tenant1/devices/device1").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo2",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // read, must have changed
        let resp = test::TestRequest::get().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({"tenant_id": "tenant1", "id": "device1", "data": {"credentials": [
            {"pass": "foo2"}
        ]}}));

        // delete, must succeed
        let resp = test::TestRequest::delete().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // read, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // update non existing device
        let resp = test::TestRequest::put().uri("/api/v1/tenants/tenant1/devices/device1").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo2",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // delete, second time, must result in "not found"
        let resp = test::TestRequest::delete().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    })
}

#[actix_rt::test]
#[serial]
async fn test_delete_tenant_deletes_device() -> anyhow::Result<()> {
    test!(app => {

        // create tenant
        let resp = test::TestRequest::post().uri("/api/v1/tenants").set_json(&json!({
            "tenant_id": "tenant1",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // create device
        let resp = test::TestRequest::post().uri("/api/v1/tenants/tenant1/devices").set_json(&json!({
            "tenant_id": "tenant1",
            "device_id": "device1",
            "password": "foo",
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // delete tenant, must succeed
        let resp = test::TestRequest::delete().uri("/api/v1/tenants/tenant1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // read device, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/v1/tenants/tenant1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    })
}
