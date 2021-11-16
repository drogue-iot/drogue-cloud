mod messages;
mod route;
mod service;
mod wshandler;

use crate::service::Service;
use actix::Actor;
use actix_web::{web, App, HttpServer};
use drogue_cloud_service_api::{auth::user::authz::Permission, kafka::KafkaClientConfig};
use drogue_cloud_service_common::{
    actix_auth::authentication::AuthN,
    actix_auth::authorization::AuthZ,
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::AuthenticatorConfig,
};
use futures::TryFutureExt;

use serde::Deserialize;
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

    pub registry: RegistryConfig,

    pub oauth: AuthenticatorConfig,
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let enable_api_keys = config.enable_api_keys;

    log::info!("Starting WebSocket integration service endpoint");
    log::info!("Kafka servers: {}", config.kafka.bootstrap_servers);

    // set up authentication

    let client = reqwest::Client::new();

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let user_auth = UserAuthClient::from_config(client.clone(), user_auth).await?;
        Some(user_auth)
    } else {
        None
    };

    let registry = config.registry.into_client(client.clone()).await?;

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
                    .wrap(AuthZ {
                        client: user_auth.clone(),
                        permission: Permission::Read,
                        app_param: "application".to_string(),
                    })
                    .wrap(AuthN {
                        openid: authenticator.as_ref().cloned(),
                        token: user_auth.clone(),
                        enable_api_key: enable_api_keys,
                    })
                    .service(route::start_connection),
            )
    })
    .bind(config.bind_addr)?
    .run();

    log::info!("RUNNING WS");

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
