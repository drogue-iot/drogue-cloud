mod common;

use drogue_client::registry;
use drogue_cloud_device_state_service::{
    app,
    service::{DeviceState, Id, InitResponse, PingResponse},
};
use drogue_cloud_service_api::webapp::test::{read_body_json, TestRequest};
use drogue_cloud_test_common::call::{call_http, user};
use http::StatusCode;
use lazy_static::lazy_static;
use serial_test::serial;
use std::collections::HashMap;
use tokio::time::sleep;

lazy_static! {
    static ref REGISTRY: HashMap<String, registry::v1::Application> = {
        let mut m = HashMap::new();
        m.insert(
            "app1".into(),
            registry::v1::Application {
                ..Default::default()
            },
        );
        m
    };
}

#[actix_rt::test]
#[serial]
async fn test_init() -> anyhow::Result<()> {
    test!((REGISTRY.clone() => app, _service, _pool, sink) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/sessions")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        // ping -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/sessions/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert!(response.lost_ids.is_empty());

        // check events
        let events = sink.events().await;
        assert_eq!(events.len(), 0);
    })
}

#[actix_rt::test]
#[serial]
async fn test_no_init() -> anyhow::Result<()> {
    test!((REGISTRY.clone() => app, _service, _pool, sink) => {
        let instance = uuid::Uuid::new_v4().to_string();
        // ping without init -> must fail
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/sessions/{}", instance))).await;
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);

        // check events
        let events = sink.events().await;
        assert_eq!(events.len(), 0);
    })
}

#[actix_rt::test]
#[serial]
async fn test_timeout() -> anyhow::Result<()> {
    test!((REGISTRY.clone() => app, _service, _pool, sink) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/sessions")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        sleep(std::time::Duration::from_secs(10)).await;

        // ping after timeout -> must fail
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/sessions/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);

        // check events
        let events = sink.events().await;
        assert_eq!(events.len(), 0);
    })
}

#[actix_rt::test]
#[serial]
async fn test_create() -> anyhow::Result<()> {
    test!((REGISTRY.clone() => app, _service, _pool, sink) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/sessions")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        let application = "app1";
        let device = "device1";

        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/sessions/{}/states/{}/{}", session, application, device))
            .set_json(DeviceState{
                device_uid: "device_uid".into(),
                endpoint: "pod1".into(),
            })
        ).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // check events
        let events = sink.events().await;
        assert_eq!(events.len(), 1);
    })
}

#[actix_rt::test]
#[serial]
async fn test_lost() -> anyhow::Result<()> {
    test!((REGISTRY.clone() => app, _service, _pool, sink) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/sessions")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        let application = "app1";
        let device = "device1";
        let id = Id {
            application: application.into(), device: device.into(),
        };

        // create -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/sessions/{}/states/{}/{}", session, application, device))
            .set_json(DeviceState{
                device_uid: "device_uid".into(),
                endpoint: "pod1".into(),
            })
        ).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        // create -> must fail, but marked as lost
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/sessions/{}/states/{}/{}", session, application, device))
            .set_json(DeviceState{
                device_uid: "device_uid".into(),
                endpoint: "pod1".into(),
            })
        ).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // ping -> must succeed, and contain id1 as lost
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/sessions/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert_eq!(vec![id.clone()], response.lost_ids);

        // create -> must fail again, still marked as lost
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/sessions/{}/states/{}/{}", session, application, device))
            .set_json(DeviceState{
                device_uid: "device_uid".into(),
                endpoint: "pod1".into(),
            })
        ).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // ping -> must succeed, and still contain id1 as lost
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/sessions/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert_eq!(vec![id.clone()], response.lost_ids);

        // delete -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::delete().uri(&format!("/api/state/v1alpha1/sessions/{}/states/{}/{}", session, application, device))).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // ping -> must succeed, and contain no ids
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/sessions/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert!(response.lost_ids.is_empty());

        // create -> must succeed again
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/sessions/{}/states/{}/{}", session, application, device))
            .set_json(DeviceState{
                device_uid: "device_uid".into(),
                endpoint: "pod1".into(),
            })
        ).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // check events
        let events = sink.events().await;
        assert_eq!(events.len(), 3);
    })
}

#[actix_rt::test]
#[serial]
async fn test_timeout_lost() -> anyhow::Result<()> {
    test!((REGISTRY.clone() => app, service, _pool, sink) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/sessions")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        let application = "app1";
        let device = "device1";

        // create -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/sessions/{}/states/{}/{}", session, application, device))
            .set_json(DeviceState{
                device_uid: "device_uid".into(),
                endpoint: "pod1".into(),
            })
        ).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        drop(resp);

        // now sleep, to time out
        sleep(std::time::Duration::from_secs(10)).await;

        // ping after timeout -> must fail
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/sessions/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
        drop(resp);

        // prune
        service.prune().await?;

        // check events
        let events = sink.events().await;
        assert_eq!(events.len(), 2);
    })
}
