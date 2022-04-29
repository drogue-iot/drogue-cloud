mod common;

use drogue_cloud_device_state_service::service::{
    PostgresDeviceStateService, PostgresServiceConfiguration,
};
use drogue_cloud_device_state_service::{
    app,
    service::{InitResponse, PingResponse},
};
use drogue_cloud_service_api::webapp::test::{read_body_json, TestRequest};
use drogue_cloud_test_common::call::{call_http, user};
use http::StatusCode;
use serial_test::serial;
use tokio::time::sleep;

#[actix_rt::test]
#[serial]
async fn test_init() -> anyhow::Result<()> {
    test!((app, _pool) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/states")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        // ping -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/states/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert!(response.lost_ids.is_empty());
    })
}

#[actix_rt::test]
#[serial]
async fn test_no_init() -> anyhow::Result<()> {
    test!((app, _pool) => {
        let instance = uuid::Uuid::new_v4().to_string();
        // ping without init -> must fail
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/states/{}", instance))).await;
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
    })
}

#[actix_rt::test]
#[serial]
async fn test_timeout() -> anyhow::Result<()> {
    test!((app, _pool) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/states")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        sleep(std::time::Duration::from_secs(10)).await;

        // ping after timeout -> must fail
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/states/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
    })
}

#[actix_rt::test]
#[serial]
async fn test_create() -> anyhow::Result<()> {
    test!((app, _pool) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/states")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        let id = "id1";

        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/states/{}/{}", session, id))).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    })
}

#[actix_rt::test]
#[serial]
async fn test_lost() -> anyhow::Result<()> {
    test!((app, _pool) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/states")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        let id = "id1";

        // create -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/states/{}/{}", session, id))).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        // create -> must fail, but mark as lost
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/states/{}/{}", session, id))).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // ping -> must succeed, and contain id1 as lost
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/states/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert_eq!(vec![id.to_string()], response.lost_ids);

        // create -> must fail again, still marked as lost
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/states/{}/{}", session, id))).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // ping -> must succeed, and still contain id1 as lost
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/states/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert_eq!(vec![id.to_string()], response.lost_ids);

        // delete -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::delete().uri(&format!("/api/state/v1alpha1/states/{}/{}", session, id))).await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // ping -> must succeed, and contain no ids
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/states/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let response: PingResponse = read_body_json(resp).await;
        assert!(response.lost_ids.is_empty());

        // create -> must succeed again
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri(&format!("/api/state/v1alpha1/states/{}/{}", session, id))).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    })
}

#[actix_rt::test]
#[serial]
async fn test_timeout_lost() -> anyhow::Result<()> {
    test!((app, pool) => {
        // init -> must succeed
        let resp = call_http(&app, &user("foo"), TestRequest::put().uri("/api/state/v1alpha1/states")).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let response: InitResponse = read_body_json(resp).await;
        assert!(!response.session.is_empty());
        let session = response.session;

        sleep(std::time::Duration::from_secs(10)).await;

        // ping after timeout -> must fail
        let resp = call_http(&app, &user("foo"), TestRequest::post().uri(&format!("/api/state/v1alpha1/states/{}", session))).await;
        assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);

        // prune
        let service = PostgresDeviceStateService::for_testing(pool);
        service.prune().await?;
    })
}
