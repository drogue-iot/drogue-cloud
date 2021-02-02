use log::LevelFilter;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[macro_export]
macro_rules! test {
   ($v:ident => $($code:block)*) => {{
        common::init();

        let cli = client();
        let db = db(&cli, |pg| service::AuthenticationServiceConfig{
            pg
        })?;

        let data = WebData {
            service: service::PostgresAuthenticationService::new(db.config.clone()).unwrap(),
        };

        let mut $v =
            test::init_service(drogue_cloud_authentication_service::app!(data, 16 * 1024)).await;

        $($code)*

        Ok(())
    }};
}

#[macro_export]
macro_rules! test_auth {
    ($rep:expr => $res:expr) => {
        test!(app => {
            let resp = test::TestRequest::post().uri("/api/v1/auth").set_json(&$rep).send_request(&mut app).await;
            let is_success = resp.status().is_success();
            let result: serde_json::Value = test::read_body_json(resp).await;

            let outcome = $res;

            assert_eq!(result, json!({"outcome": outcome}));
            assert!(is_success);
        })
    };
}
