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
async fn test_create_app() -> anyhow::Result<()> {
    test!(app => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers().get(header::LOCATION), Some(&HeaderValue::from_static("http://localhost:8080/api/v1/apps/app1")));
    })
}

/// Create, update, and delete an app. Check the current state and the operation outcomes.
#[actix_rt::test]
#[serial]
async fn test_crud_app() -> anyhow::Result<()> {
    test!(app => {

        // try read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // create, must succeed
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({
            "metadata": {
                "name": "app1"
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

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
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

        // try read, must not exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // second delete, must report "not found"
        let resp = test::TestRequest::delete().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    })
}

/// Create, update, and delete an app. Check the current state and the operation outcomes.
#[actix_rt::test]
#[serial]
async fn test_app_labels() -> anyhow::Result<()> {
    test!(app => {

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

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
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

        // read, must exist
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
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
    test!(app => {
        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp = test::TestRequest::post().uri("/api/v1/apps").set_json(&json!({
            "metadata": {
                "name": "app1",
            },
        })).send_request(&mut app).await;

        assert_eq!(resp.status(), StatusCode::CONFLICT);
    })
}

#[actix_rt::test]
#[serial]
async fn test_app_trust_anchor() -> anyhow::Result<()> {
    let ca = include_bytes!("certs/ca-cert.pem").to_vec();
    let ca = base64::encode(ca);

    test!(app => {
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

        // read, must exist, with cert
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
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
                            "certificate": ca,
                            "notBefore": "2021-02-02T08:39:17Z",
                            "notAfter": "2031-01-31T08:39:17Z",
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

        // read, must exist, but no cert
        let resp = test::TestRequest::get().uri("/api/v1/apps/app1").send_request(&mut app).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let result: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(result, json!({
            "metadata": {
                "name": "app1",
            }
        }));
    })
}
