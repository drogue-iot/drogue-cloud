mod common;

use crate::common::{assert_events, init, outbox_retrieve};
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
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/v1/apps/app1")));

        // an event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Application {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
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
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![]);
    })
}

/// Try some cases of "bad input data"
#[actix_rt::test]
#[serial]
async fn test_create_device_bad_request() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't check application events this time
        sender.reset()?;
        outbox_retrieve(&outbox).await?;

        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "" // empty name
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // no event must have been fired
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_create_duplicate_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't check application events this time
        sender.reset()?;
        outbox_retrieve(&outbox).await?;

        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            }
        })).send_request(&mut app).await;

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
    test!((app, sender, outbox) => {

        // read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create app first
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        // we don't test for application events this time
        sender.reset()?;
        outbox_retrieve(&outbox).await?;

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
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // read, must exist now
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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
        assert_events(vec![sender.retrieve()?, outbox_retrieve(&outbox).await?], vec![Event::Device {
            instance: "drogue-instance".into(),
            application: "app1".into(),
            device: "device1".into(),
            uid: "".into(),
            path: ".spec.credentials".into(),
            generation: 0,
        }]);

        // read, must have changed
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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
                }
            }
        }));

        // delete, must succeed
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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
async fn test_delete_app_deletes_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {

        // create tenant
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

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
        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
            },
        })).send_request(&mut app).await;

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
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1").send_request(&mut app).await;
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
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    })
}

#[actix_rt::test]
#[serial]
async fn test_delete_app_finalizer_device() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {

        // create tenant
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

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
        let resp = test::TestRequest::post().uri("/api/v1/apps/app1/devices").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "finalizers": ["foo"], // create a device with finalizer
            },
        })).send_request(&mut app).await;

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
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1").send_request(&mut app).await;
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
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;

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
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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

        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1/devices/device1").set_json(&json!({
            "metadata": {
                "application": "app1",
                "name": "device1",
                "finalizers": [],
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // no event must have been fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);

        // read device, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // read app, must no longer not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    })
}

#[actix_rt::test]
#[serial]
async fn test_lock_device_resource_version() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

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

        // get current state

        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1/devices/device1").set_json(&update1).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // update device twice (using previous version) ... must fail
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1/devices/device1").set_json(&update2).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

    })
}

#[actix_rt::test]
#[serial]
async fn test_lock_device_uid() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

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

        // get current state

        let resp = test::TestRequest::get().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;

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
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1/devices/device1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // recreate
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

        // update device (using previous version) ... must fail
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1/devices/device1").set_json(&update).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

    })
}
