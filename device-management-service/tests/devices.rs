mod common;

use crate::common::{
    assert_events, assert_resources, call_http, create_app, create_device, init, outbox_retrieve,
    user,
};
use actix_cors::Cors;
use actix_http::Request;
use actix_web::{
    body::MessageBody,
    dev::{Service, ServiceResponse},
    http::StatusCode,
    middleware::Condition,
    test, web, App, Error,
};
use drogue_cloud_admin_service::apps;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self},
    WebData,
};
use drogue_cloud_registry_events::{mock::MockEventSender, Event};
use drogue_cloud_service_api::auth::user::UserInformation;
use drogue_cloud_test_common::{client, db};
use http::{header, HeaderValue};
use maplit::hashmap;
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_create_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/registry/v1alpha1/apps/app1")));

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Application {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "name": "device1",
                "application": "app1"
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        { "pass": "foo" }
                    ]
                },
                "alias": ["baz", "42", "waldo"]
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/registry/v1alpha1/apps/app1/devices/device1")));

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

    })
}

#[actix_rt::test]
#[serial]
async fn test_create_device_no_app() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "name": "device1",
                "application": "app1"
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({"error": "ReferenceNotFound", "message": "Referenced a non-existing entity"}));

        // no event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![]);
    })
}

/// Try some cases of "bad input data"
#[actix_rt::test]
#[serial]
async fn test_create_device_bad_request() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't check application events this time
        sender.reset()?;
        outbox_retrieve(&outbox).await?;

        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "" // empty name
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // no event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_create_duplicate_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't check application events this time
        sender.reset()?;
        outbox_retrieve(&outbox).await?;

        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_crud_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {

        // read, must not exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create app first
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't test for application events this time
        sender.reset()?;
        outbox_retrieve(&outbox).await?;

        // read, must still not exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create device
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
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
                },
                "alias": ["baz", "42", "waldo"]
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // read, must exist now
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();
        let generation = result["metadata"]["generation"].clone();
        let uid = result["metadata"]["uid"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "uid": uid,
                "creationTimestamp": creation_timestamp,
                "generation": generation,
                "resourceVersion": resource_version,
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                        {"user": {"username": "foo", "password": "bar"}}
                    ]
                },
                "alias": ["baz", "42", "waldo"]
            }
        }));

        // update device
        let resp = test::TestRequest::put().uri("/api/registry/v1alpha1/apps/app1/devices/device1").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                    ]
                },
                "alias": ["baz"]
            }
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![
        Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".spec.alias".into(),
            generation: 0,
        },
        Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".spec.credentials".into(),
            generation: 0,
        }]);

        // read, must have changed
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let new_resource_version = result["metadata"]["resourceVersion"].clone();
        let new_generation = result["metadata"]["generation"].clone();

        assert_ne!(resource_version, new_resource_version);
        assert_ne!(generation, new_generation);

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "uid": uid,
                "creationTimestamp": creation_timestamp,
                "generation": new_generation,
                "resourceVersion": new_resource_version,
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                    ]
                },
                "alias": ["baz"]
            }
        }));

        // delete, must succeed
        let resp = test::TestRequest::delete().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // read, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // update non existing device
        let resp = test::TestRequest::put().uri("/api/registry/v1alpha1/apps/app1/devices/device1").set_json(&json!({
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
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);

        // delete, second time, must result in "not found"
        let resp = test::TestRequest::delete().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_delete_app_deletes_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {

        // create tenant
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Application {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // create device
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // delete application, must succeed
        let resp = test::TestRequest::delete().uri("/api/registry/v1alpha1/apps/app1").send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // one event must have been fired for the application, the device event is omitted as the
        // devices  doesn't have a finalizer set
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Application {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // read device, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    })
}

#[actix_rt::test]
#[serial]
async fn test_delete_app_finalizer_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {

        // create tenant
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Application {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // create device
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "finalizers": ["foo"], // create a device with finalizer
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // delete application, must succeed
        let resp = test::TestRequest::delete().uri("/api/registry/v1alpha1/apps/app1").send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // one event must have been fired, but notify about a metadata change
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Application {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            uid: "".into(),
            path: ".metadata".into(),
            generation: 0,
        }]);

        // the application must still exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let deletion_timestamp = result["metadata"]["deletionTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();
        let generation = result["metadata"]["generation"].clone();
        let uid = result["metadata"]["uid"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "uid": uid,
                "creationTimestamp": creation_timestamp,
                "generation": generation,
                "resourceVersion": resource_version,
                "deletionTimestamp": deletion_timestamp,
                "finalizers": ["has-devices"],
            },
        }));

        // read device, must still exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();
        let generation = result["metadata"]["generation"].clone();
        let uid = result["metadata"]["uid"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "uid": uid,
                "creationTimestamp": creation_timestamp,
                "generation": generation,
                "resourceVersion": resource_version,
                "finalizers": ["foo"]
            },
        }));

        // now delete the device, must succeed, but soft deleted

        let resp = test::TestRequest::delete().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired

        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".metadata".into(),
            generation: 0,
        }]);

        // read device, must still exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let deletion_timestamp = result["metadata"]["deletionTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();
        let generation = result["metadata"]["generation"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "uid": uid,
                "creationTimestamp": creation_timestamp,
                "generation": generation,
                "resourceVersion": resource_version,
                "deletionTimestamp": deletion_timestamp,
                "finalizers": ["foo"]
            },
        }));

        // update device, remove finalizer
        let resp = test::TestRequest::put().uri("/api/registry/v1alpha1/apps/app1/devices/device1").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "finalizers": [],
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);

        // read device, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // read app, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1").send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    })
}

#[actix_rt::test]
#[serial]
async fn test_lock_device_resource_version() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
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
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // get current state

        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();
        let generation = result["metadata"]["generation"].clone();
        let uid = result["metadata"]["uid"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "uid": uid,
                "creationTimestamp": creation_timestamp,
                "generation": generation,
                "resourceVersion": resource_version,
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                    ]
                }
            }
        }));

        // remember result as update for next step

        let mut update1 = result.clone();
        let update2 = result;
        update1["spec"]["credentials"] = json!({});

        // update device once (using current version) ... must succeed
        let resp = test::TestRequest::put().uri("/api/registry/v1alpha1/apps/app1/devices/device1").set_json(&update1).send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // update device twice (using previous version) ... must fail
        let resp = test::TestRequest::put().uri("/api/registry/v1alpha1/apps/app1/devices/device1").set_json(&update2).send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

    })
}

#[actix_rt::test]
#[serial]
async fn test_lock_device_uid() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
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
        })).send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // get current state

        let resp = test::TestRequest::get().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();
        let generation = result["metadata"]["generation"].clone();
        let uid = result["metadata"]["uid"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "uid": uid,
                "creationTimestamp": creation_timestamp,
                "generation": generation,
                "resourceVersion": resource_version,
            },
            "spec": {
                "credentials": {
                    "credentials": [
                        {"pass": "foo"},
                    ]
                }
            }
        }));

        // remember result as update for next step
        let update = result.clone();

        // delete, must succeed
        let resp = test::TestRequest::delete().uri("/api/registry/v1alpha1/apps/app1/devices/device1").send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // recreate
        let resp = test::TestRequest::post().uri("/api/registry/v1alpha1/apps/app1/devices").set_json(&json!({
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
        })).send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // update device (using previous version) ... must fail
        let resp = test::TestRequest::put().uri("/api/registry/v1alpha1/apps/app1/devices/device1").set_json(&update).send_request(&app).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

    })
}

#[actix_rt::test]
#[serial]
async fn test_search_devices() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {

        let foo = user("foo");

        create_app(&app, &foo, "my-app", hashmap!(
        )).await?;

        create_device(&app, &foo, "my-app", "device-1", hashmap!(
        )).await?;

        create_device(&app, &foo, "my-app", "device-2", hashmap!(
            "floor" => "1",
            "room" => "101",
        )).await?;

        create_device(&app, &foo, "my-app", "device-3", hashmap!(
            "floor" => "1",
            "room" => "102",
        )).await?;

        create_device(&app, &foo, "my-app", "device-4", hashmap!(
            "floor" => "1",
            "room" => "102",
            "important" => "",
        )).await?;

        create_device(&app, &foo, "my-app", "device-5", hashmap!(
            "floor" => "2",
            "room" => "201",
        )).await?;

        assert_devices(&app, &foo, "my-app", Some("floor"), None, None, &["device-2", "device-3", "device-4", "device-5"]).await?;
        assert_devices(&app, &foo, "my-app", Some("!floor"), None, None, &["device-1"]).await?;

        assert_devices(&app, &foo, "my-app", Some("floor=1"), None, None, &["device-2", "device-3", "device-4"]).await?;
        assert_devices(&app, &foo, "my-app", Some("floor!=1"), None, None, &["device-5"]).await?;

        assert_devices(&app, &foo, "my-app", Some("important"), None, None, &["device-4"]).await?;
        assert_devices(&app, &foo, "my-app", Some("floor in (1,2)"), None, None, &["device-2", "device-3", "device-4", "device-5"]).await?;
        assert_devices(&app, &foo, "my-app", Some("floor notin (1,2)"), None, None, &[]).await?;

        assert_devices(&app, &foo, "my-app", Some("floor"), Some(2), None, &["device-2", "device-3"]).await?;
        assert_devices(&app, &foo, "my-app", Some("floor"), Some(2), Some(1), &[ "device-3", "device-4"]).await?;
    })
}

#[allow(dead_code)]
pub async fn assert_devices<S, B, E, S1>(
    app: &S,
    user: &UserInformation,
    app_name: S1,
    labels: Option<&str>,
    limit: Option<usize>,
    offset: Option<usize>,
    outcome: &[&str],
) -> anyhow::Result<()>
where
    S: Service<Request, Response = ServiceResponse<B>, Error = E>,
    B: MessageBody + Unpin,
    E: std::fmt::Debug,
    S1: AsRef<str>,
    B::Error: Into<Error>,
{
    let query = {
        let mut query = form_urlencoded::Serializer::new(String::new());
        if let Some(labels) = labels {
            query.append_pair("labels", labels);
        }
        if let Some(limit) = limit {
            query.append_pair("limit", &limit.to_string());
        }
        if let Some(offset) = offset {
            query.append_pair("offset", &offset.to_string());
        }
        query.finish()
    };

    let resp = call_http(
        app,
        &user,
        test::TestRequest::get().uri(&format!(
            "/api/registry/v1alpha1/apps/{}/devices?{}",
            app_name.as_ref(),
            query
        )),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    let result: serde_json::Value = test::read_body_json(resp).await;
    assert_resources(result, outcome);

    Ok(())
}
