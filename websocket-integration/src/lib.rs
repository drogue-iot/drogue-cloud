mod messages;
mod route;
mod service;
mod wshandler;

use crate::service::Service;
use actix::Actor;
use actix_web::web;
use drogue_client::user::v1::authz::Permission;
use drogue_cloud_service_api::{
    kafka::KafkaClientConfig,
    webapp::{self as actix_web},
};
use drogue_cloud_service_common::{
    actix::http::{HttpBuilder, HttpConfig},
    actix_auth::{authentication::AuthN, authorization::AuthZ},
    app::Startup,
    auth::openid,
    auth::pat,
    client::{RegistryConfig, UserAuthClientConfig},
    defaults,
};
use lazy_static::lazy_static;
use prometheus::{labels, opts, register_int_gauge, IntGauge};
use serde::Deserialize;
use std::collections::HashMap;

lazy_static! {
    pub static ref CONNECTIONS_COUNTER: IntGauge = register_int_gauge!(opts!(
        "drogue_connections",
        "Connections",
        labels! {
                "protocol" => "ws",
                "type" => "integration"
        }
    ))
    .unwrap();
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    #[serde(default)]
    pub kafka: KafkaClientConfig,

    pub registry: RegistryConfig,

    pub oauth: openid::AuthenticatorConfig,

    #[serde(default)]
    pub http: HttpConfig,
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    let enable_access_token = config.enable_access_token;

    log::info!("Starting WebSocket integration service endpoint");
    log::info!("Kafka servers: {}", config.kafka.bootstrap_servers);

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        Some(user_auth.into_client().await?)
    } else {
        None
    };

    let registry = config.registry.into_client().await?;

    // create and start the service actor
    let service_addr = Service {
        clients: HashMap::default(),
        kafka_config: config.kafka,
        registry,
    }
    .start();
    let service_addr = web::Data::new(service_addr);

    // main server

    HttpBuilder::new(config.http, Some(startup.runtime_config()), move |cfg| {
        cfg.app_data(service_addr.clone());
        if let Some(authenticator) = authenticator.clone() {
            cfg.app_data(authenticator);
        }
        if let Some(user_auth) = user_auth.clone() {
            cfg.app_data(user_auth);
        }

        cfg.service(
            web::scope("/{application}")
                .wrap(AuthZ {
                    client: user_auth.clone(),
                    permission: Permission::Read,
                    app_param: "application".to_string(),
                })
                .wrap(AuthN {
                    openid: authenticator.as_ref().cloned(),
                    token: user_auth.clone().map(|u| pat::Authenticator::new(u)),
                    enable_access_token,
                })
                .service(web::resource("").route(web::get().to(route::start_connection))),
        );
    })
    .start(startup)?;

    // exiting
    Ok(())
}
