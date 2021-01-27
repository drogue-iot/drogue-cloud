use actix_cors::Cors;
use actix_web::{middleware::Condition, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self, PostgresManagementService},
    WebData,
};
use drogue_cloud_service_common::openid::AuthenticatorError;
use drogue_cloud_test_common::{client, db};
use log::LevelFilter;
use serde_json::json;
use serial_test::serial;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[macro_export]
macro_rules! test {
   ($v:ident => $($code:block)*) => {{
        init();

        let cli = client();
        let db = db(&cli, |pg| service::ManagementServiceConfig{
            pg
        })?;

        let data = web::Data::new(WebData {
            authenticator: drogue_cloud_service_common::openid::Authenticator { client: None, scopes: "".into() },
            service: service::PostgresManagementService::new(db.config.clone()).unwrap(),
        });

        let mut $v =
            actix_web::test::init_service(app!(data, false, 16 * 1024)).await;

        $($code)*

        Ok(())
    }};
}

#[actix_rt::test]
#[serial]
async fn test_health() -> anyhow::Result<()> {
    test!(app => {
        let req = actix_web::test::TestRequest::get().uri("/health").to_request();
        let resp: serde_json::Value = actix_web::test::read_response_json(&mut app, req).await;

        assert_eq!(resp, json!({"success": true}));
    })
}
