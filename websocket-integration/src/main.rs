mod auth;
mod messages;
mod route;
mod service;
mod wshandler;

use dotenv::dotenv;

use actix::Actor;
use actix_web::{web, App, HttpServer};

use drogue_cloud_service_common::{
    config::ConfigFromEnv, health::HealthServer, openid::Authenticator,
};
use drogue_cloud_service_common::{defaults, health::HealthServerConfig};
use serde::Deserialize;

use crate::service::Service;
use drogue_cloud_service_common::client::{UserAuthClient, UserAuthClientConfig};
use drogue_cloud_service_common::openid::TokenConfig;
use futures::TryFutureExt;
use std::sync::Arc;

use drogue_client::registry;
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use std::collections::HashMap;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,
    #[serde(default)]
    pub disable_api_keys: bool,

    #[serde(default)]
    pub health: HealthServerConfig,

    user_auth: UserAuthClientConfig,

    #[serde(default)]
    pub kafka: KafkaClientConfig,
    #[serde(default)]
    pub registry: RegistryConfig,
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

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env().unwrap();

    let enable_auth = config.enable_auth;

    log::info!("Starting WebSocket integration service endpoint");
    log::info!("Authentication enabled: {}", enable_auth);
    log::info!("Kafka servers: {}", config.kafka.bootstrap_servers);

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

    let auth = web::Data::new(authenticator);
    let authz = web::Data::new(user_auth);
    let enable_api_keys = web::Data::new(config.disable_api_keys);

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

    // create and start the service actor
    let service_addr = Service {
        clients: HashMap::default(),
        kafka_config: config.kafka,
        registry,
    }
    .start();
    let service_addr = web::Data::new(service_addr);

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main server

    let main = HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            //.wrap(Condition::new(enable_auth, bearer_auth.clone()))
            .app_data(service_addr.clone())
            .app_data(auth.clone())
            .app_data(authz.clone())
            .app_data(enable_api_keys.clone())
            .service(route::start_connection)
    })
    .bind(config.bind_addr)?
    .run();

    // run
    futures::try_join!(health.run(), main.err_into())?;

    // exiting
    Ok(())
}
