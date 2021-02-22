mod common;

use crate::common::init;
use actix_cors::Cors;
use actix_web::{middleware::Condition, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self, PostgresManagementService},
    WebData,
};
use drogue_cloud_registry_events::mock::MockEventSender;
use drogue_cloud_service_common::openid::AuthenticatorError;
use drogue_cloud_test_common::{client, db};
use serde_json::json;
use serial_test::serial;

#[actix_rt::test]
#[serial]
async fn test_health() -> anyhow::Result<()> {
    test!((app, _sender) => {
        let req = actix_web::test::TestRequest::get().uri("/health").to_request();
        let resp: serde_json::Value = actix_web::test::read_response_json(&mut app, req).await;

        assert_eq!(resp, json!({"success": true}));
    })
}
