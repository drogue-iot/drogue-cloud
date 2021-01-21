use actix_cors::Cors;
use actix_web::{middleware::Condition, web, App};
use actix_web_httpauth::middleware::HttpAuthentication;
use deadpool::managed::{PoolConfig, Timeouts};
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self, PostgresManagementService},
    WebData,
};
use drogue_cloud_service_common::openid::AuthenticatorError;
use log::LevelFilter;
use serde_json::json;
use serial_test::serial;
use std::{env, time::Duration};
use testcontainers::{
    clients,
    images::generic::{GenericImage, WaitFor},
    Container, Docker,
};

pub struct PostgresRunner<'c, C: Docker> {
    pub config: service::ManagementServiceConfig,
    db: Container<'c, C, GenericImage>,
}

impl<'c, C: Docker> PostgresRunner<'c, C> {
    pub fn new(cli: &'c C, config: service::ManagementServiceConfig) -> anyhow::Result<Self> {
        log::info!("Starting postgres");

        let db = cli.run(
            GenericImage::new("docker.io/library/postgres:12")
                .with_mapped_port((5432, 5432))
                .with_env_var("POSTGRES_PASSWORD", "mysecretpassword")
                .with_volume(
                    env::current_dir()?
                        .join("sql")
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("Failed to generate SQL path"))?,
                    "/docker-entrypoint-initdb.d",
                )
                .with_wait_for(WaitFor::message_on_stdout(
                    "[1] LOG:  database system is ready to accept connections", // listening on pid 1
                )),
        );

        // sleep(time::Duration::from_secs(1));

        Ok(Self { config, db })
    }
}

impl<'c, C: Docker> Drop for PostgresRunner<'c, C> {
    fn drop(&mut self) {
        log::info!("Stopping postgres");
        self.db.stop();
    }
}

pub fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

pub fn db<C: Docker>(cli: &C) -> anyhow::Result<PostgresRunner<C>> {
    let config = service::ManagementServiceConfig {
        pg: deadpool_postgres::Config {
            host: Some("localhost".into()),
            user: Some("postgres".into()),
            password: Some("mysecretpassword".into()),
            dbname: Some("postgres".into()),

            pool: Some(PoolConfig {
                max_size: 15,
                timeouts: Timeouts {
                    wait: Some(Duration::from_secs(5)),
                    ..Default::default()
                },
            }),

            ..Default::default()
        },
    };

    Ok(PostgresRunner::new(cli, config)?)
}

#[macro_export]
macro_rules! test {
   ($v:ident => $($code:block)*) => {{
        init();

        let cli = clients::Cli::default();
        let db = db(&cli)?;

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
