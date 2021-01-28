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
