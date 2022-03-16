mod service;

pub use crate::service::ServiceConfig;

use drogue_cloud_endpoint_common::{
    sender::{ExternalClientPoolConfig, UpstreamSender},
    sink::KafkaSink,
};
use drogue_cloud_mqtt_common::server::{build, MqttServerOptions, TlsConfig};
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_common::{
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    defaults,
    health::{HealthServer, HealthServerConfig},
    metrics,
    openid::AuthenticatorConfig,
    reqwest::ClientFactory,
};
use futures::TryFutureExt;
use lazy_static::lazy_static;
use prometheus::{IntGauge, Opts};
use serde::Deserialize;
use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

lazy_static! {
    pub static ref CONNECTIONS_COUNTER: IntGauge = IntGauge::with_opts(
        Opts::new("drogue_connections", "Connections")
            .const_label("protocol", "mqtt")
            .const_label("type", "integration")
    )
    .unwrap();
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub disable_tls: bool,
    #[serde(default)]
    pub disable_client_certificates: bool,

    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
    #[serde(default)]
    pub mqtt: MqttServerOptions,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub service: ServiceConfig,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub oauth: AuthenticatorConfig,

    pub command_kafka_sink: KafkaClientConfig,

    #[serde(default = "defaults::check_kafka_topic_ready")]
    pub check_kafka_topic_ready: bool,

    #[serde(default = "defaults::instance")]
    pub instance: String,

    #[serde(default)]
    pub endpoint_pool: ExternalClientPoolConfig,
}

impl TlsConfig for Config {
    fn is_disabled(&self) -> bool {
        self.disable_tls
    }

    fn disable_client_certs(&self) -> bool {
        self.disable_client_certificates
    }

    fn key_file(&self) -> Option<&str> {
        self.key_file.as_deref()
    }

    fn cert_bundle_file(&self) -> Option<&str> {
        self.cert_bundle_file.as_deref()
    }
}

#[derive(Clone)]
pub struct OpenIdClient {
    pub client: openid::Client,
}

impl Debug for OpenIdClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OpenIdClient")
            .field("client", &"...")
            .finish()
    }
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::debug!("Config: {:#?}", config);

    let app_config = config.clone();

    log::info!(
        "User/password enabled: {}",
        config.service.enable_username_password_auth
    );
    log::info!("Kafka servers: {}", config.service.kafka.bootstrap_servers);

    // set up security

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let user_auth = Arc::new(UserAuthClient::from_config(user_auth).await?);
        Some(user_auth)
    } else {
        None
    };

    let registry = config.registry.into_client().await?;

    let sender = UpstreamSender::new(
        config.instance,
        KafkaSink::from_config(config.command_kafka_sink, config.check_kafka_topic_ready)?,
        config.endpoint_pool,
    )?;

    log::info!("Authenticator: {:?}", authenticator);
    log::info!("User auth: {:?}", user_auth);

    // creating the application

    let app = service::App {
        authenticator,
        user_auth,
        config: config.service.clone(),
        sender,
        client: ClientFactory::new().build()?,
        registry,
    };

    // create server

    let srv = build(config.mqtt.clone(), app, &app_config)?.run();

    log::info!("Starting server");

    // run

    if let Some(health) = config.health {
        metrics::register(Box::new(CONNECTIONS_COUNTER.clone()))?;
        // health server
        let health =
            HealthServer::new(health, vec![], Some(prometheus::default_registry().clone()));
        futures::try_join!(health.run_ntex(), srv.err_into(),)?;
    } else {
        futures::try_join!(srv)?;
    }

    // exiting

    Ok(())
}
