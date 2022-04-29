use log::LevelFilter;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[macro_export]
macro_rules! test {
    (($app:ident, $pool:ident) => $($code:tt)*) => {{

        use drogue_cloud_service_api::webapp::*;
        use drogue_cloud_device_state_service::service::{self, DeviceStateService};
        use std::sync::Arc;

        common::init();

        let cli = drogue_cloud_test_common::client();

        let db = drogue_cloud_test_common::db(&cli, |pg| drogue_cloud_device_state_service::service::PostgresServiceConfiguration {
            pg,
        })?;

        let $pool = db.config.pg.create_pool(tokio_postgres::NoTls)?;

        let auth = drogue_cloud_service_common::mock_auth!();

        let service = service::PostgresDeviceStateService::new(db.config.clone())?;
        let service: Arc<dyn DeviceStateService> = Arc::new(service);
        let service: web::Data<dyn DeviceStateService> = web::Data::from(service);

        let $app = drogue_cloud_service_api::webapp::test::init_service(
            app!(service, 16 * 1024, auth, None)
                .wrap_fn(|req, srv|{
                    log::warn!("Running test-user middleware");
                    use drogue_cloud_service_api::webapp::dev::Service;
                    use drogue_cloud_service_api::webapp::HttpMessage;
                    {
                        let user: Option<&drogue_cloud_service_api::auth::user::UserInformation> = req.app_data();
                        if let Some(user) = user {
                            log::warn!("Replacing user with test-user: {:?}", user);
                            req.extensions_mut().insert(user.clone());
                        }
                    }
                    srv.call(req)
                })
        )
        .await;

        $($code)*;

        Ok(())
    }};
}
