#![type_length_limit = "6000000"]

mod error;
mod mqtt;
mod server;
mod service;

use crate::server::{build, build_tls};
use crate::service::ServiceConfig;
use dotenv::dotenv;
use drogue_cloud_service_common::client::{UserAuthClient, UserAuthClientConfig};
use drogue_cloud_service_common::config::ConfigFromEnv;
use drogue_cloud_service_common::endpoints::eval_endpoints;
use drogue_cloud_service_common::openid::{create_client, Authenticator, AuthenticatorConfig};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub enable_auth: bool,
    #[serde(default)]
    pub disable_tls: bool,
    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,
    #[serde(default)]
    pub bind_addr_mqtt: Option<String>,

    pub service: ServiceConfig,
    pub user_auth: UserAuthClientConfig,
}

#[derive(Clone)]
pub struct OpenIdClient {
    pub client: openid::Client,
    pub scopes: String,
}

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;

    let enable_auth = config.enable_auth;

    log::info!("Authentication enabled: {}", enable_auth);

    let endpoints = eval_endpoints().await?;

    let openid_client = if enable_auth {
        let config = AuthenticatorConfig::from_env()?;
        Some(OpenIdClient {
            client: create_client(&config, endpoints.clone()).await?,
            scopes: config.scopes,
        })
    } else {
        None
    };

    let authenticator = openid_client
        .as_ref()
        .map(|client| Authenticator::from_client(client.client.clone()));

    let user_auth = openid_client
        .as_ref()
        .map(|client| UserAuthClient::from_openid_client(&config.user_auth, client.client.clone()))
        .transpose()?
        .map(Arc::new);

    let app = service::App {
        authenticator,
        user_auth,
        config: config.service.clone(),
    };

    let builder = ntex::server::Server::build();
    let addr = config.bind_addr_mqtt.as_deref();

    let builder = if !config.disable_tls {
        build_tls(addr, builder, app, &config)?
    } else {
        build(addr, builder, app)?
    };

    log::info!("Starting server");

    builder.run().await?;

    Ok(())
}
