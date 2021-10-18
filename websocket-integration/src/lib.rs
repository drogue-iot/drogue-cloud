mod messages;
mod route;
mod service;
mod wshandler;

use actix::Actor;
use actix_web::{web, App, HttpServer};

use drogue_cloud_service_common::{defaults, health::HealthServerConfig};
use drogue_cloud_service_common::{health::HealthServer, openid::Authenticator};
use serde::Deserialize;

use crate::service::Service;
use anyhow::Context;
use drogue_cloud_service_common::client::{RegistryConfig, UserAuthClient, UserAuthClientConfig};
use futures::TryFutureExt;

use drogue_cloud_service_api::auth::user::authz::Permission;
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_common::actix_auth::Auth;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default = "defaults::enable_api_keys")]
    pub enable_api_keys: bool,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    #[serde(default)]
    pub kafka: KafkaClientConfig,

    #[serde(default)]
    pub registry: Option<RegistryConfig>,
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let enable_api_keys = config.enable_api_keys;

    log::info!("Starting WebSocket integration service endpoint");
    log::info!("Kafka servers: {}", config.kafka.bootstrap_servers);

    // set up authentication

    let (authenticator, user_auth) = if let Some(user_auth) = config.user_auth {
        let client = reqwest::Client::new();
        let authenticator = Authenticator::new().await?;
        let user_auth = UserAuthClient::from_config(client, user_auth).await?;
        (Some(authenticator), Some(user_auth))
    } else {
        (None, None)
    };

    let client = reqwest::Client::new();
    let registry = config
        .registry
        .context("no registry configured")?
        .into_client(client.clone())
        .await?;

    // create and start the service actor
    let service_addr = Service {
        clients: HashMap::default(),
        kafka_config: config.kafka,
        registry,
    }
    .start();
    let service_addr = web::Data::new(service_addr);

    // main server

    let main = HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(service_addr.clone())
            .service(
                web::scope("/{application}")
                    .wrap(Auth {
                        auth_n: authenticator.clone(),
                        auth_z: user_auth.clone(),
                        permission: Some(Permission::Read),
                        enable_api_key: enable_api_keys,
                    })
                    .service(route::start_connection),
            )
    })
    .bind(config.bind_addr)?
    .run();

    // run
    if let Some(health) = config.health {
        let health = HealthServer::new(health, vec![]);
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    // exiting
    Ok(())
}
