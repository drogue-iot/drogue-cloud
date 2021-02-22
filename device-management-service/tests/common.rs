use log::LevelFilter;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[macro_export]
macro_rules! test {
   (($app:ident, $sender:ident) => $($code:block)*) => {{
        init();

        let cli = client();
        let db = db(&cli, |pg| service::PostgresManagementServiceConfig {
            pg,
            instance: "drogue-instance".to_string(),
        })?;

        let sender = MockEventSender::new();

        let data = web::Data::new(WebData {
            authenticator: drogue_cloud_service_common::openid::Authenticator { client: None, scopes: "".into() },
            service: service::PostgresManagementService::new(db.config.clone(), sender.clone()).unwrap(),
        });

        let mut $sender = sender;

        let mut $app =
            actix_web::test::init_service(app!(MockEventSender, data, false, 16 * 1024)).await;

        $($code)*

        Ok(())
    }};
}
