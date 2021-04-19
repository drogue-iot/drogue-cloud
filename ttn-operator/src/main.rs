mod controller;
mod data;
mod endpoints;
mod error;
mod ttn;

use actix_web::{
    get, middleware,
    web::{self},
    App, HttpResponse, HttpServer, Responder,
};
use anyhow::anyhow;
use dotenv::dotenv;
use drogue_client::registry;
use drogue_cloud_service_common::endpoints::create_endpoint_source;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::TokenConfig,
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default)]
    pub registry: RegistryConfig,

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

pub struct WebData {
    pub controller: controller::Controller,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let endpoint_source = create_endpoint_source()?;
    let endpoints = endpoint_source.eval_endpoints().await?;

    let endpoint_url = endpoints
        .http
        .map(|http| http.url)
        .ok_or_else(|| anyhow!("Missing HTTP endpoint information"))
        .and_then(|url| Ok(Url::parse(&url)?))?
        .join("/ttn/v3")?;

    let client = reqwest::Client::new();
    let controller = controller::Controller::new(
        registry::v1::Client::new(
            client.clone(),
            config.registry.url,
            Some(
                TokenConfig::from_env_prefix("REGISTRY")?
                    .amend_with_env()
                    .discover_from(client.clone())
                    .await?,
            ),
        ),
        ttn::Client::new(client),
        endpoint_url,
    );

    let data = web::Data::new(WebData { controller });

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main

    let main = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(data.clone())
            .service(index)
            .service(endpoints::events)
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
