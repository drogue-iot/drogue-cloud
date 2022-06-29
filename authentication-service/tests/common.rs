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
        let db = db(&cli, |pg| service::AuthenticationServiceConfig { pg }).unwrap();

        let data = web::Data::new(WebData {
            authenticator: None,
            service: service::PostgresAuthenticationService::new(db.config.clone()).unwrap(),
        });

        let auth = drogue_cloud_service_common::mock_auth!();

        let $v = actix_web::test::init_service({
            let app = App::new();
            app.configure(|cfg| {
                drogue_cloud_authentication_service::app!(cfg, data, false, auth);
            })
        })
        .await;

        $code;
    }};
}

#[macro_export]
macro_rules! test_auth {
    ($rep:expr => $res:expr) => {
        test!(app => {
            let resp = actix_web::test::TestRequest::post().uri("/api/v1/auth").set_json(&$rep).send_request(&app).await;
            let is_success = resp.status().is_success();

            println!("Response: {:?}", resp);

            let result: serde_json::Value = actix_web::test::read_body_json(resp).await;

            let outcome = $res;

            assert_eq!(result, json!({"outcome": outcome}));
            assert!(is_success);
        })
    };
}
