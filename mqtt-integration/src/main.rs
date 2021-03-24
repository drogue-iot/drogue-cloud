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
use drogue_cloud_service_common::{
    client::{UserAuthClient, UserAuthClientConfig},
    config::ConfigFromEnv,
    defaults,
    openid::{Authenticator, TokenConfig},
};
use serde::Deserialize;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

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

    pub max_size: Option<u32>,

    pub service: ServiceConfig,
    pub user_auth: UserAuthClientConfig,
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

    log::info!("Authentication enabled: {}", enable_auth);
    log::info!(
        "User/password enabled: {}",
        config.service.enable_username_password_auth
    );
    log::info!("Kafka servers: {}", config.service.kafka_bootstrap_servers);
    log::info!("Kafka topic: {}", config.service.kafka_topic);

    let (openid_client, authenticator, user_auth) = if enable_auth {
        let client = TokenConfig::from_env()?.into_client(None).await?;
        (
            match config.service.enable_username_password_auth {
                true => Some(OpenIdClient {
                    client: client.clone(),
                }),
                false => None,
            },
            Some(Authenticator::new().await?),
            Some(Arc::new(UserAuthClient::from_openid_client(
                &config.user_auth,
                client,
            )?)),
        )
    } else {
        (None, None, None)
    };

    let app = service::App {
        authenticator,
        user_auth,
        openid_client,
        config: config.service.clone(),
    };

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr_mqtt.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app, &config)?
    } else {
        build(addr, builder, app, &config)?
    };

    log::info!("Starting server");

    builder.run().await?;

    Ok(())
}
