mod v1alpha1;

use actix_web::{web, HttpResponse, Responder};
use drogue_client::user::v1::authz::Permission;
use drogue_cloud_endpoint_common::{
    sender::{ExternalClientPoolConfig, UpstreamSender},
    sink::KafkaSink,
};
use drogue_cloud_service_api::{
    health::HealthChecked,
    kafka::KafkaClientConfig,
    webapp::{self as actix_web, web::ServiceConfig},
};
use drogue_cloud_service_common::{
    actix::http::{CorsBuilder, HttpBuilder, HttpConfig},
    actix_auth::authentication::AuthN,
    actix_auth::authorization::AuthZ,
    app::{Startup, StartupExt},
    auth::{
        openid::{Authenticator, AuthenticatorConfig},
        pat,
    },
    client::{RegistryConfig, UserAuthClientConfig},
    defaults,
};
use serde::Deserialize;
use serde_json::json;
use std::str;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    pub registry: RegistryConfig,

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

pub async fn configurator(
    config: Config,
) -> anyhow::Result<(
    impl Fn(&mut ServiceConfig) + Send + Sync + Clone,
    Vec<Box<dyn HealthChecked>>,
)> {
    let sender = UpstreamSender::new(
        config.instance,
        KafkaSink::from_config(config.command_kafka_sink, config.check_kafka_topic_ready)?,
        config.endpoint_pool,
    )?;

    let enable_access_token = config.enable_access_token;

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        Some(user_auth.into_client().await?)
    } else {
        None
    };

    let client = reqwest::Client::new();
    let registry = config.registry.into_client().await?;

    Ok((
        move |cfg: &mut ServiceConfig| {
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
                            token: user_auth
                                .clone()
                                .map(|user_auth| pat::Authenticator::new(user_auth)),
                            enable_access_token,
                        })
                        .route("", web::post().to(v1alpha1::command)),
                );
        },
        vec![],
    ))
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    log::info!("Starting Command service endpoint");

    // main server

    let (cfg, checks) = configurator(config.clone()).await?;
    HttpBuilder::new(config.http, Some(startup.runtime_config()), cfg)
        .cors(CorsBuilder::Permissive)
        .start(startup)?;

    // spawn

    startup.check_iter(checks);

    // exiting

    Ok(())
}
