mod common;

use actix_web::{test, web, App};
use drogue_cloud_authentication_service::{endpoints, service, WebData};
use drogue_cloud_test_common::{client, db};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_health() -> anyhow::Result<()> {
    test!(app => {
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp: serde_json::Value = test::read_response_json(&mut app, req).await;

        assert_eq!(resp, json!({"success": true}));
    })
}
