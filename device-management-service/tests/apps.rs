mod common;

use crate::common::{assert_events, init};
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
async fn test_create_app() -> anyhow::Result<()> {
    test!((app, sender, outbox) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/v1/apps/app1")));

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into(),
            generation: 0,
        }], outbox);
    })
}

/// Create, update, and delete an app. Check the current state and the operation outcomes.
#[actix_rt::test]
#[serial]
async fn test_crud_app() -> anyhow::Result<()> {
    test!((app, sender) => {

        // try read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create, must succeed, event sent
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 0,
                "resourceVersion": resource_version,
            }
        }));

        // update, must succeed
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1").set_json(&json!({
            "metadata": {
                "name": "app1"
            },
            "spec": {
                "core": {
                    "disabled": true,
                }
            }
        })).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".spec.core".into(),
            generation: 0,
        }]);

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let new_resource_version = result["metadata"]["resourceVersion"].clone();

        assert_ne!(resource_version, new_resource_version);

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 1,
                "resourceVersion": new_resource_version,
            },
            "spec": {
                "core": {
                    "disabled": true
                },
            },
        }));

        // delete, must succeed
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // try read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // second delete, must report "not found"
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // no additional event must be fired
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

/// Create, update, and delete an app. Check the current state and the operation outcomes.
#[actix_rt::test]
#[serial]
async fn test_app_labels() -> anyhow::Result<()> {
    test!((app, sender) => {

        // try read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create, must succeed
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
                "labels": {
                    "foo": "bar",
                    "foo/bar": "baz",
                },
            },
        })).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 0,
                "resourceVersion": resource_version,
                "labels": {
                    "foo": "bar",
                    "foo/bar": "baz",
                }
            }
        }));

        // update, must succeed
        let resp = test::TestRequest::put().uri("/api/v1/apps/app1").set_json(&json!({
            "metadata": {
                "name": "app1",
                "labels": {
                    "foo": "bar",
                    "baz/bar": "foo",
                }
            },
            "spec": {
                "core": {
                    "disabled": true,
                }
            }
        })).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![
            Event::Application {
                instance: "drogue-instance".into(),
                id: "app1".into(),
                path: ".metadata".into(),
                generation: 0,
            },
            Event::Application {
                instance: "drogue-instance".into(),
                id: "app1".into(),
                path: ".spec.core".into(),
                generation: 0,
            }
        ]);

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let new_resource_version = result["metadata"]["resourceVersion"].clone();

        assert_ne!(resource_version, new_resource_version);

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 1,
                "resourceVersion": new_resource_version,
                "labels": {
                    "foo": "bar",
                    "baz/bar": "foo",
                }
            },
            "spec": {
                "core": {
                    "disabled": true
                },
            },
        }));

    })
}

#[actix_rt::test]
#[serial]
async fn test_create_duplicate_app() -> anyhow::Result<()> {
    test!((app, sender) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into(),
            generation: 0,
        }]);


        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // no event for the failed attempt
        assert_eq!(sender.retrieve().unwrap(), vec![]);
    })
}

#[actix_rt::test]
#[serial]
async fn test_app_trust_anchor() -> anyhow::Result<()> {
    let ca = include_bytes!("certs/ca-cert.pem").to_vec();
    let ca = base64::encode(ca);

    test!((app, sender) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
            "spec": {
                "trustAnchors": {
                    "anchors": [
                        { "certificate": ca, }
                    ],
                }
            }
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into(),
            generation: 0,
        }]);

        // read, must exist, with cert
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 0,
                "resourceVersion": resource_version,
            },
            "spec": {
                "trustAnchors": {
                    "anchors": [
                        { "certificate": ca }
                    ]
                }
            },
            "status": {
                "trustAnchors": {
                    "anchors": [ {
                        "valid": {
                            "subject": "O=Drogue IoT, OU=Cloud, CN=Application 1",
                            "notBefore": "2021-02-02T11:11:31Z",
                            "notAfter": "2031-01-31T11:11:31Z",
                            "certificate": ca,
                        }
                    }]
                }
            }
        }));

        // drop cert

        let resp = test::TestRequest::put().uri("/api/v1/apps/app1").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // two events must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".spec.trustAnchors".into(),
            generation: 0,
        }, Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".status.trustAnchors".into(),
            generation: 0,
        }]);

        // read, must exist, but no cert
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 1,
                "resourceVersion": resource_version,
            }
        }));
    })
}

#[actix_rt::test]
#[serial]
async fn test_delete_finalizer() -> anyhow::Result<()> {
    test!((app, sender) => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
                "finalizers": ["foo", "bar"],
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".".into(),
            generation: 0,
        }]);

        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1").send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // an event must have been fired
        assert_events(sender.retrieve().unwrap(), vec![Event::Application {
            instance: "drogue-instance".into(),
            id: "app1".into(),
            path: ".metadata".into(),
            generation: 0,
        }]);

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let mut result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();
        let deletion_timestamp = result["metadata"]["deletionTimestamp"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 1,
                "resourceVersion": resource_version,
                "deletionTimestamp": deletion_timestamp,
                "finalizers": ["foo", "bar"],
            }
        }));

        // delete one finalizer
        result["metadata"]["finalizers"] = json!(["bar"]);

        let resp = test::TestRequest::put().uri("/api/v1/apps/app1").set_json(&result).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // get another metadata event
        assert_events(sender.retrieve().unwrap(), vec![
            Event::Application {
                instance: "drogue-instance".into(),
                id: "app1".into(),
                path: ".metadata".into(),
                generation: 0,
            },
        ]);

        // read, must exist (one less finalizer, some deletion timestamp)
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let mut result: serde_json::Value = test::read_body_json(resp).await;

        let creation_timestamp = result["metadata"]["creationTimestamp"].clone();
        let resource_version = result["metadata"]["resourceVersion"].clone();

        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
                "creationTimestamp": creation_timestamp,
                "generation": 2,
                "resourceVersion": resource_version,
                "deletionTimestamp": deletion_timestamp,
                "finalizers": ["bar"],
            }
        }));

        // delete last finalizer
        result["metadata"]["finalizers"] = json!([]);

        let resp = test::TestRequest::put().uri("/api/v1/apps/app1").set_json(&result).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // no more events when cleaning up after finalizers
        assert_eq!(sender.retrieve().unwrap(), vec![]);

    })
}
