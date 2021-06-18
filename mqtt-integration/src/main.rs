#![type_length_limit = "6000000"]

mod error;
mod mqtt;
mod server;
mod service;

use crate::{
    server::{build, build_tls},
    service::ServiceConfig,
};
use dotenv::dotenv;
use drogue_client::registry;
use drogue_cloud_endpoint_common::downstream::{DownstreamSender, KafkaSink};
use drogue_cloud_service_common::{
    client::{UserAuthClient, UserAuthClientConfig},
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::{Authenticator, TokenConfig},
};
use futures::TryFutureExt;
use serde::Deserialize;
use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,
    #[serde(default)]
    pub disable_tls: bool,
    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
    #[serde(default)]
    pub bind_addr_mqtt: Option<String>,

    #[serde(default)]
    pub registry: RegistryConfig,

    pub max_size: Option<u32>,

    #[serde(default)]
    pub service: ServiceConfig,
    pub user_auth: UserAuthClientConfig,

    #[serde(default)]
    pub health: HealthServerConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RegistryConfig {
    #[serde(default = "defaults::registry_url")]
    pub url: Url,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: defaults::registry_url(),
        }
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

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;

    let enable_auth = config.enable_auth;
    let app_config = config.clone();

    log::info!("Authentication enabled: {}", enable_auth);
    log::info!(
        "User/password enabled: {}",
        config.service.enable_username_password_auth
    );
    log::info!("Kafka servers: {}", config.service.kafka_bootstrap_servers);
    log::info!("Kafka topic: {}", config.service.kafka_topic);

    // set up security

    let (authenticator, user_auth) = if enable_auth {
        let client = reqwest::Client::new();
        let authenticator = Authenticator::new().await?;
        let user_auth = Arc::new(
            UserAuthClient::from_config(
                client,
                config.user_auth,
                TokenConfig::from_env_prefix("USER_AUTH")?.amend_with_env(),
            )
            .await?,
        );
        (Some(authenticator), Some(user_auth))
    } else {
        (None, None)
    };

    let client = reqwest::Client::new();

    let registry = registry::v1::Client::new(
        client.clone(),
        config.registry.url,
        Some(
            TokenConfig::from_env_prefix("REGISTRY")?
                .amend_with_env()
                .discover_from(client.clone())
                .await?,
        ),
    );
    let sender = DownstreamSender::new(KafkaSink::new("COMMAND_KAFKA_SINK")?)?;

    // creating the application

    let app = service::App {
        authenticator,
        user_auth,
        config: config.service.clone(),
        sender,
        client,
        registry,
    };

    // start building the server

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr_mqtt.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app, &app_config)?
    } else {
        build(addr, builder, app, &app_config)?
    };

    // health server

    let health = HealthServer::new(config.health, vec![]);

    log::info!("Starting server");

    // run

    futures::try_join!(health.run_ntex(), builder.run().err_into(),)?;

    // exiting

    Ok(())
}
