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
use drogue_cloud_registry_events::{mock::MockEventSender, Event};
use drogue_cloud_service_common::openid::AuthenticatorError;
use drogue_cloud_test_common::{client, db};
use http::{header, HeaderValue};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_create_device() -> anyhow::Result<()> {
    test!((app, sender) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/v1/apps/app1")));

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into()
        }]);

        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "name": "device1",
                "application": "app1"
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        { "pass": "foo" }
                    ]
                }
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/v1/apps/app1/devices/device1")));

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            id: "device1".into(),
            path: ".".into()
        }]);

    })
}

#[actix_rt::test]
#[serial]
async fn test_create_device_no_tenant() -> anyhow::Result<()> {
    test!((app, sender) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "name": "device1",
                "application": "app1"
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({"error": "ReferenceNotFound", "message": "Referenced a non-existing entity"}));

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

/// Try some cases of "bad input data"
#[actix_rt::test]
#[serial]
async fn test_create_device_bad_request() -> anyhow::Result<()> {
    test!((app, sender) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't check application events this time
        sender.reset()?;

        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "" // empty name
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_create_duplicate_device() -> anyhow::Result<()> {
    test!((app, sender) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't check application events this time
        sender.reset()?;

        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            id: "device1".into(),
            path: ".".into()
        }]);

        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_crud_device() -> anyhow::Result<()> {
    test!((app, sender) => {

        // read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create tenant first
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't test for application events this time
        sender.reset()?;

        // read, must still not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create device
        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1"
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                        {"user": {"username": "foo", "password": "bar"}}
                    ]
                }
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            id: "device1".into(),
            path: ".".into()
        }]);

        // read, must exist now
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "creationTimestamp": creation_timestamp,
                "generation": 0,
                "resourceVersion": resource_version,
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                        {"user": {"username": "foo", "password": "bar"}}
                    ]
                }
            }
        }));

        // update device
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1/devices/device1").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                    ]
                }
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            id: "device1".into(),
            path: ".spec.credentials".into()
        }]);

        // read, must have changed
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let new_resource_version = result["metadata"]["resourceVersion"].clone();

        assert_ne!(resource_version, new_resource_version);

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "creationTimestamp": creation_timestamp,
                "generation": 1,
                "resourceVersion": new_resource_version,
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                    ]
                }
            }
        }));

        // delete, must succeed
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            id: "device1".into(),
            path: ".".into()
        }]);

        // read, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // update non existing device
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1/devices/device1").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                    ]
                }
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);

        // delete, second time, must result in "not found"
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_delete_tenant_deletes_device() -> anyhow::Result<()> {
    test!((app, sender) => {

        // create tenant
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into()
        }]);

        // create device
        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            id: "device1".into(),
            path: ".".into()
        }]);

        // delete tenant, must succeed
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // two events must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into()
        }, Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            id: "device1".into(),
            path: ".".into()
        }]);

        // read device, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    })
}
