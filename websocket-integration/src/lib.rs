mod messages;
mod route;
mod service;
mod wshandler;

use crate::service::Service;
use actix::Actor;
use actix_web::{web, App, HttpServer};
use drogue_cloud_service_api::{
    auth::user::authz::Permission,
    kafka::KafkaClientConfig,
    webapp::{self as actix_web, prom::PrometheusMetricsBuilder},
};
use drogue_cloud_service_common::{
    actix::bind_http,
    actix_auth::{authentication::AuthN, authorization::AuthZ},
    app::run_main,
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    defaults,
    health::HealthServerConfig,
    openid::AuthenticatorConfig,
    tls::{TlsMode, WithTlsMode},
};
use futures::{FutureExt, TryFutureExt};
use lazy_static::lazy_static;
use prometheus::{labels, opts, register_int_gauge, IntGauge};
use serde::Deserialize;
use std::collections::HashMap;

lazy_static! {
    pub static ref CONNECTIONS_COUNTER: IntGauge = register_int_gauge!(opts!(
        "drogue_connections",
        "Connections",
        labels! {
                "protocol" => "ws",
                "type" => "integration"
        }
    ))
    .unwrap();
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::max_payload_size")]
    pub max_payload_size: usize,
    #[serde(default)]
    pub disable_tls: bool,
    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,

    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    #[serde(default)]
    pub kafka: KafkaClientConfig,

    pub registry: RegistryConfig,

    pub oauth: AuthenticatorConfig,

    #[serde(default)]
    pub workers: Option<usize>,
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let enable_access_token = config.enable_access_token;

    log::info!("Starting WebSocket integration service endpoint");
    log::info!("Kafka servers: {}", config.kafka.bootstrap_servers);

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let user_auth = UserAuthClient::from_config(user_auth).await?;
        Some(user_auth)
    } else {
        None
    };

    let registry = config.registry.into_client().await?;

    // create and start the service actor
    let service_addr = Service {
        clients: HashMap::default(),
        kafka_config: config.kafka,
        registry,
    }
    .start();
    let service_addr = web::Data::new(service_addr);

    let max_payload_size = config.max_payload_size;
    let max_json_payload_size = config.max_json_payload_size;

    let prometheus = PrometheusMetricsBuilder::new("http_endpoint")
        .registry(prometheus::default_registry().clone())
        .build()
        .unwrap();

    // main server

    let main = HttpServer::new(move || {
        App::new()
            .wrap(prometheus.clone())
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::PayloadConfig::new(max_payload_size))
            .app_data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(service_addr.clone())
            .app_data(authenticator.clone())
            .app_data(user_auth.clone())
            .service(
                web::scope("/{application}")
                    .wrap(AuthZ {
                        client: user_auth.clone(),
                        permission: Permission::Read,
                        app_param: "application".to_string(),
                    })
                    .wrap(AuthN {
                        openid: authenticator.as_ref().cloned(),
                        token: user_auth.clone(),
                        enable_access_token,
                    })
                    .service(web::resource("").route(web::get().to(route::start_connection))),
            )
    });

    let mut main = bind_http(
        main,
        config.bind_addr,
        config.disable_tls.with_tls_mode(TlsMode::NoClient),
        config.key_file,
        config.cert_bundle_file,
    )?;

    if let Some(workers) = config.workers {
        main = main.workers(workers)
    }

    let main = main.run().err_into().boxed_local();

    // run

    run_main([main], config.health, vec![]).await?;

    // exiting
    Ok(())
}
