use log::LevelFilter;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[macro_export]
macro_rules! test {
    ($v:ident => $code:tt) => {{
        common::init();

        let cli = client();
        let db = db(&cli, |pg| service::AuthorizationServiceConfig { pg }).expect("Init database");

        let data = web::Data::new(WebData {
            authenticator: None,
            service: service::PostgresAuthorizationService::new(db.config.clone()).unwrap(),
        });
        let api_key = web::Data::new(drogue_cloud_api_key_service::endpoints::WebData {
            service: drogue_cloud_api_key_service::mock::MockApiKeyService,
        });

        let auth = drogue_cloud_service_common::mock_auth!();
        let $v = test::init_service(drogue_cloud_user_auth_service::app!(
            data,
            drogue_cloud_api_key_service::mock::MockApiKeyService,
            api_key,
            16 * 1024,
            false,
            auth
        ))
        .await;

        $code;
    }};
}

#[macro_export]
macro_rules! test_auth {
    ($rep:expr => $res:expr) => {
        test!(app => {
            let resp = test::TestRequest::post().uri("/api/v1/user/authz").set_json(&$rep).send_request(&app).await;
            let is_success = resp.status().is_success();

            log::debug!("Response: {:?}", resp);

            let result: serde_json::Value = test::read_body_json(resp).await;

            let outcome = $res;

            assert_eq!(result, json!({"outcome": outcome}));
            assert!(is_success);
        })
    };
}
