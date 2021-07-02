mod v1alpha1;

use actix_cors::Cors;
use actix_web::{
    get,
    middleware::{self, Condition},
    web, App, HttpResponse, HttpServer, Responder,
};
use dotenv::dotenv;
use drogue_client::registry;
use drogue_cloud_endpoint_common::downstream::{DownstreamSender, KafkaSink};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::{Authenticator, TokenConfig},
    openid_auth,
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use std::str;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,

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

#[derive(Debug)]
pub struct WebData {
    pub authenticator: Option<Authenticator>,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting Command service endpoint");

    let sender = DownstreamSender::new(KafkaSink::new("COMMAND_KAFKA_SINK")?)?;

    let config = Config::from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let data = web::Data::new(WebData { authenticator });

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

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData>>()
            .as_ref()
            .and_then(|d|d.authenticator.as_ref())
        });
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(data.clone())
            .app_data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(sender.clone())
            .app_data(registry.clone())
            .app_data(client.clone())
            .service(index)
            .service(
                web::scope("/api/command/v1alpha1")
                    .wrap(Condition::new(enable_auth, auth))
                    .wrap(Cors::permissive())
                    .service(
                        web::resource("/apps/{appId}/devices/{deviceId}")
                            .route(web::post().to(v1alpha1::command::<KafkaSink>)),
                    ),
            )
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
