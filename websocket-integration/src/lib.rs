mod messages;
mod route;
mod service;
mod wshandler;

use crate::service::Service;
use actix::Actor;
use actix_web::{web, App, HttpServer};
use drogue_cloud_service_api::{
    auth::user::authz::Permission, kafka::KafkaClientConfig, webapp as actix_web,
};
use drogue_cloud_service_common::{
    actix_auth::authentication::AuthN,
    actix_auth::authorization::AuthZ,
    app::run_main,
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    defaults,
    health::HealthServerConfig,
    metrics,
    openid::AuthenticatorConfig,
};
use lazy_static::lazy_static;
use prometheus::{IntGauge, Opts};
use serde::Deserialize;
use std::collections::HashMap;

lazy_static! {
    pub static ref CONNECTIONS_COUNTER: IntGauge = IntGauge::with_opts(
        Opts::new("drogue_connections", "Connections")
            .const_label("protocol", "ws")
            .const_label("type", "integration")
    )
    .unwrap();
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    #[serde(default)]
    pub kafka: KafkaClientConfig,

    pub registry: RegistryConfig,

    pub oauth: AuthenticatorConfig,

    #[serde(default)]
    pub workers: Option<usize>,
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let enable_access_token = config.enable_access_token;

    log::info!("Starting WebSocket integration service endpoint");
    log::info!("Kafka servers: {}", config.kafka.bootstrap_servers);

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let user_auth = UserAuthClient::from_config(user_auth).await?;
        Some(user_auth)
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
                        enable_access_token,
                    })
                    .service(web::resource("").route(web::get().to(route::start_connection))),
            )
    })
    .bind(config.bind_addr)?;

    let main = if let Some(workers) = config.workers {
        main.workers(workers).run()
    } else {
        main.run()
    };

    // run

    metrics::register(Box::new(CONNECTIONS_COUNTER.clone()))?;
    run_main(main, config.health, vec![]).await?;

    // exiting
    Ok(())
}
