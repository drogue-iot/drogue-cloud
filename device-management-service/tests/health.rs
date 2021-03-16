mod common;

use crate::common::init;
use actix_cors::Cors;
use actix_web::{middleware::Condition, web, App};
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self},
    WebData,
};
use drogue_cloud_registry_events::mock::MockEventSender;
use drogue_cloud_test_common::{client, db};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_health() -> anyhow::Result<()> {
    test!((app, _sender, _outbox) => {
        let req = actix_web::test::TestRequest::get().uri("/health").to_request();
        let resp: serde_json::Value = actix_web::test::read_response_json(&app, req).await;

        assert_eq!(resp, json!({"success": true}));
    })
}
