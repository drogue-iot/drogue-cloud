mod common;

use crate::common::{call_http, create_app, init, user};
use actix_cors::Cors;
use actix_web::{http::StatusCode, test, web, App};
use drogue_cloud_admin_service::apps;
use drogue_cloud_device_management_service::crud;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self},
    WebData,
};
use drogue_cloud_registry_events::mock::MockEventSender;
use drogue_cloud_service_common::keycloak::{
    mock::KeycloakAdminMock, KeycloakAdminClientConfig, KeycloakClient,
};
use drogue_cloud_test_common::{client, db};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_transfer_app() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let foo = user("foo");
        let bar = user("bar");

        create_app(&app, &foo, "app1", Default::default()).await?;

        // get as user "foo" - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // get as user "bar" - must fail

        let resp = call_http(&app, &bar, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // transfer app - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership").set_json(&json!({
            "newUser": "bar",
        }))).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        // accept app - must succeed

        let resp = call_http(&app, &bar, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/accept-ownership")).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // get as user "foo" - must fail now

        let resp = call_http(&app, &foo, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // get as user "bar" - must succeed now

        let resp = call_http(&app, &bar, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::OK);

    })
}

#[actix_rt::test]
#[serial]
async fn test_transfer_cancel() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let foo = user("foo");
        let bar = user("bar");

        create_app(&app, &foo, "app1", Default::default()).await?;

        // get as user "foo" - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // get as user "bar" - must fail

        let resp = call_http(&app, &bar, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // transfer app - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership").set_json(&json!({
            "newUser": "bar",
        }))).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        // cancel transfer - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::delete().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership")).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // accept app - must fail

        let resp = call_http(&app, &bar, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/accept-ownership")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // get as user "foo" - must still succeed

        let resp = call_http(&app, &foo, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // get as user "bar" - must still fail

        let resp = call_http(&app, &bar, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    })
}

#[actix_rt::test]
#[serial]
async fn test_transfer_app_fails() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let foo = user("foo");
        let bar = user("bar");

        create_app(&app, &foo, "app1", Default::default()).await?;

        // get as user "foo"

        let resp = call_http(&app, &foo, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;

        // must succeed

        assert_eq!(resp.status(), StatusCode::OK);

        // get as user "bar"

        let resp = call_http(&app, &bar, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;

        // must fail

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // try stealing app - by transferring

        let resp = call_http(&app, &bar, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership").set_json(&json!({
            "newUser": "bar",
        }))).await;

        // must fail

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // try stealing app - by accepting

        let resp = call_http(&app, &bar, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/accept-ownership")).await;

        // must fail

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // try canceling a transfer - without having permission

        let resp = call_http(&app, &bar, test::TestRequest::delete().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership")).await;

        // must fail

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    })
}

#[actix_rt::test]
#[serial]
async fn test_decline_transfer_app() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let foo = user("foo");
        let bar = user("bar");
        let baz = user("baz");

        create_app(&app, &foo, "app1", Default::default()).await?;

        // get as user "foo" - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // transfer app - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership").set_json(&json!({
            "newUser": "bar",
        }))).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        // decline transfer as user "bar" - must succeed

        let resp = call_http(&app, &bar, test::TestRequest::delete().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership")).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // accept transfer as user "bar" - must fail (transfer no longer exit)

        let resp = call_http(&app, &foo, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/accept-ownership")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    })
}

#[actix_rt::test]
#[serial]
async fn test_read_app_transfer_state() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let foo = user("foo");
        let bar = user("bar");
        let baz = user("baz");


        create_app(&app, &foo, "app1", Default::default()).await?;

        // get as user "foo" - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // transfer app - must succeed

        let resp = call_http(&app, &foo, test::TestRequest::put().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership").set_json(&json!({
            "newUser": "bar",
        }))).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        // read transfer state - must succeed

        let resp = call_http(&app, &bar, test::TestRequest::get().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // read transfer state as user "bar" - must succeed

        let resp = call_http(&app, &bar, test::TestRequest::get().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // read transfer state as user "baz" - must fail

        let resp = call_http(&app, &baz, test::TestRequest::get().uri("/api/admin/v1alpha1/apps/app1/transfer-ownership")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    })
}
