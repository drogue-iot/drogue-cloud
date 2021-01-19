use actix_web::{test, web, App};
use deadpool::managed::{PoolConfig, Timeouts};
use drogue_cloud_authentication_service::{endpoints, service, WebData};
use drogue_cloud_service_api::{AuthenticationRequest, Credential};
use log::LevelFilter;
use serde_json::json;
use serial_test::serial;
use std::{env, time::Duration};
use testcontainers::images::generic::WaitFor;
use testcontainers::{clients, images::generic::GenericImage, Container, Docker};

pub struct PostgresRunner<'c, C: Docker> {
    pub config: service::AuthenticationServiceConfig,
    db: Container<'c, C, GenericImage>,
}

impl<'c, C: Docker> PostgresRunner<'c, C> {
    pub fn new(cli: &'c C, config: service::AuthenticationServiceConfig) -> anyhow::Result<Self> {
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

fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(LevelFilter::Debug)
        .try_init();
}

fn db<C: Docker>(cli: &C) -> anyhow::Result<PostgresRunner<C>> {
    let config = service::AuthenticationServiceConfig {
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

macro_rules! test {
   ($v:ident => $($code:block)*) => {{
        init();

        let cli = clients::Cli::default();
        let db = db(&cli)?;

        let data = WebData {
            service: service::PostgresAuthenticationService::new(db.config.clone()).unwrap(),
        };

        let mut $v =
            test::init_service(drogue_cloud_authentication_service::app!(data, 16 * 1024)).await;

        $($code)*

        Ok(())
    }};
}

macro_rules! test_auth {
    ($rep:expr => $res:expr) => {
        test!(app => {
            let resp = test::TestRequest::post().uri("/api/v1/auth").set_json(&$rep).send_request(&mut app).await;
            let is_success = resp.status().is_success();
            let result: serde_json::Value = test::read_body_json(resp).await;

            assert_eq!(result, $res);
            assert!(is_success);
        })
    };
}

#[actix_rt::test]
#[serial]
async fn test_health() -> anyhow::Result<()> {
    test!(app => {
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp: serde_json::Value = test::read_response_json(&mut app, req).await;

        assert_eq!(resp, json!({"success": true}));
    })
}

#[actix_rt::test]
#[serial]
async fn test_auth_tenant() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
        tenant: "tenant1".into(),
        device: "device1".into(),
        credential: Credential::Password("foo".into())
    } => json!({"pass":{
        "tenant": {"id": "tenant1", "data": {}},
        "device": {"tenant_id": "tenant1", "id": "device1", "data": {}}}
    }))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_wrong_password() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "device1".into(),
            credential: Credential::Password("foo1".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_tenant() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant2".into(),
            device: "device1".into(),
            credential: Credential::Password("foo".into())
    } => json!("fail"))
}

#[actix_rt::test]
#[serial]
async fn test_auth_fails_missing_device() -> anyhow::Result<()> {
    test_auth!(AuthenticationRequest{
            tenant: "tenant1".into(),
            device: "device2".into(),
            credential: Credential::Password("foo".into())
    } => json!("fail"))
}
