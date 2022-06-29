mod v1alpha1;

use actix_web::{web, HttpResponse, Responder};
use drogue_cloud_endpoint_common::{
    sender::{ExternalClientPoolConfig, UpstreamSender},
    sink::KafkaSink,
};
use drogue_cloud_service_api::{
    auth::user::authz::Permission, kafka::KafkaClientConfig, webapp as actix_web,
};
use drogue_cloud_service_common::{
    actix::{CorsBuilder, HttpBuilder, HttpConfig},
    actix_auth::authentication::AuthN,
    actix_auth::authorization::AuthZ,
    app::run_main,
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    openid::AuthenticatorConfig,
};
use drogue_cloud_service_common::{defaults, health::HealthServerConfig, openid::Authenticator};
use serde::Deserialize;
use serde_json::json;
use std::str;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    pub oauth: AuthenticatorConfig,

    pub command_kafka_sink: KafkaClientConfig,

    #[serde(default = "defaults::check_kafka_topic_ready")]
    pub check_kafka_topic_ready: bool,

    #[serde(default = "defaults::instance")]
    pub instance: String,

    #[serde(default)]
    pub endpoint_pool: ExternalClientPoolConfig,

    #[serde(default)]
    pub http: HttpConfig,
}

#[derive(Debug)]
pub struct WebData {
    pub authenticator: Option<Authenticator>,
}

async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::info!("Starting Command service endpoint");

    let sender = UpstreamSender::new(
        config.instance,
        KafkaSink::from_config(config.command_kafka_sink, config.check_kafka_topic_ready)?,
        config.endpoint_pool,
    )?;

    let enable_access_token = config.enable_access_token;

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let user_auth = UserAuthClient::from_config(user_auth).await?;
        Some(user_auth)
    } else {
        None
    };

    let client = reqwest::Client::new();
    let registry = config.registry.into_client().await?;

    // main server

    let main = HttpBuilder::new(config.http, move |cfg| {
        cfg.app_data(web::Data::new(sender.clone()))
            .app_data(web::Data::new(registry.clone()))
            .app_data(web::Data::new(client.clone()))
            .service(web::resource("/").route(web::get().to(index)))
            .service(
                web::scope("/api/command/v1alpha1/apps/{application}/devices/{deviceId}")
                    .wrap(AuthZ {
                        client: user_auth.clone(),
                        permission: Permission::Write,
                        app_param: "application".to_string(),
                    })
                    .wrap(AuthN {
                        openid: authenticator.as_ref().cloned(),
                        token: user_auth.clone(),
                        enable_access_token,
                    })
                    .route("", web::post().to(v1alpha1::command)),
            );
    })
    .cors(CorsBuilder::Permissive)
    .run()?;

    // run

    run_main([main], config.health, vec![]).await?;

    // exiting

    Ok(())
}
