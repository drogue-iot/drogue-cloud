mod v1alpha1;

use actix_cors::Cors;
use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};
use drogue_cloud_endpoint_common::{sender::UpstreamSender, sink::KafkaSink};
use drogue_cloud_service_common::{
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::Authenticator,
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use std::str;

use drogue_cloud_service_api::auth::user::authz::Permission;
use drogue_cloud_service_common::{
    actix_auth::Auth,
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    openid::AuthenticatorConfig,
};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::enable_api_keys")]
    pub enable_api_keys: bool,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    pub oauth: AuthenticatorConfig,
}

#[derive(Debug)]
pub struct WebData {
    pub authenticator: Option<Authenticator>,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::info!("Starting Command service endpoint");

    let sender = UpstreamSender::new(KafkaSink::new("COMMAND_KAFKA_SINK")?)?;

    let max_json_payload_size = config.max_json_payload_size;

    let enable_api_keys = config.enable_api_keys;

    let client = reqwest::Client::new();

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let user_auth = UserAuthClient::from_config(client.clone(), user_auth).await?;
        Some(user_auth)
    } else {
        None
    };

    let client = reqwest::Client::new();
    let registry = config.registry.into_client(client.clone()).await?;

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
                web::scope("/api/command/v1alpha1/apps/{application}/devices/{deviceId}")
                    .wrap(Auth {
                        auth_n: authenticator.as_ref().cloned(),
                        auth_z: user_auth.clone(),
                        permission: Some(Permission::Write),
                        enable_api_key: enable_api_keys,
                    })
                    .wrap(Cors::permissive())
                    .route("", web::post().to(v1alpha1::command::<KafkaSink>)),
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
