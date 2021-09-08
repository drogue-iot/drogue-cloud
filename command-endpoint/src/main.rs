mod v1alpha1;

use actix_cors::Cors;
use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};
use dotenv::dotenv;
use drogue_client::registry;
use drogue_cloud_endpoint_common::{sender::UpstreamSender, sink::KafkaSink};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::{Authenticator, TokenConfig},
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use std::str;
use url::Url;

use drogue_cloud_service_api::auth::user::authz::Permission;
use drogue_cloud_service_common::actix_auth::Auth;
use drogue_cloud_service_common::client::{UserAuthClient, UserAuthClientConfig};

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,
    #[serde(default = "defaults::enable_api_keys")]
    pub enable_api_keys: bool,

    #[serde(default)]
    pub registry: RegistryConfig,

    #[serde(default)]
    pub health: HealthServerConfig,

    user_auth: UserAuthClientConfig,
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

    let sender = UpstreamSender::new(KafkaSink::new("COMMAND_KAFKA_SINK")?)?;

    let config = Config::from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let enable_auth = config.enable_auth;
    let enable_api_keys = config.enable_api_keys;

    // set up authentication

    let (authenticator, user_auth) = if enable_auth {
        let client = reqwest::Client::new();
        let authenticator = Authenticator::new().await?;
        let user_auth = UserAuthClient::from_config(
            client,
            config.user_auth,
            TokenConfig::from_env_prefix("USER_AUTH")?.amend_with_env(),
        )
        .await?;
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

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main server

    let main = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(web::Data::new(sender.clone()))
            .app_data(web::Data::new(registry.clone()))
            .app_data(web::Data::new(client.clone()))
            .service(index)
            .service(
                web::scope("/api/command/v1alpha1")
                    .wrap(Cors::permissive())
                    .service(
                        web::scope("/apps/{application}/devices/{deviceId}")
                            .wrap(Auth {
                                auth_n: authenticator.clone(),
                                auth_z: user_auth.clone(),
                                permission: Permission::Write,
                                enable_api_key: enable_api_keys,
                            })
                            .route("", web::post().to(v1alpha1::command::<KafkaSink>)),
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
