use log::LevelFilter;

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

#[macro_export]
macro_rules! test {
    (($registry:expr => $app:ident, $service:ident, $pool:ident, $sink:ident) => $($code:tt)*) => {{

        use drogue_cloud_service_api::webapp::*;
        use drogue_cloud_device_state_service::service::{self, DeviceStateService};
        use std::sync::Arc;
        use drogue_cloud_endpoint_common::sender::DownstreamSender;

        common::init();

        let cli = drogue_cloud_test_common::client();

        let db = drogue_cloud_test_common::db(&cli, |pg| service::postgres::PostgresServiceConfiguration {
            pg,
            session_timeout: std::time::Duration::from_secs(10),
        })?;

        let $pool = db.config.pg.create_pool()?;

        let auth = drogue_cloud_service_common::mock_auth!();

        let $sink = drogue_cloud_test_common::sink::MockSink::new();
        let sender = DownstreamSender::new($sink.clone(), "drogue".to_string(), Default::default()).unwrap();

        let service = service::postgres::PostgresDeviceStateService::new(db.config.clone(), sender, $registry)?;
        let $service = service.clone();

        let s: Arc<dyn DeviceStateService> = Arc::new(service);
        let s: web::Data<dyn DeviceStateService> = web::Data::from(s);

        let $app = drogue_cloud_service_api::webapp::test::init_service(
            app!(s, 16 * 1024, auth, None)
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
