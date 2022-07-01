pub mod endpoints;
pub mod service;

use crate::service::{postgres::PostgresServiceConfiguration, DeviceStateService};
use actix_web::web;
use drogue_cloud_endpoint_common::{
    sender::{DownstreamSender, ExternalClientPoolConfig},
    sink::KafkaSink,
};
use drogue_cloud_service_api::{
    health::HealthChecked,
    kafka::KafkaClientConfig,
    webapp::{self as actix_web},
};
use drogue_cloud_service_common::{
    actix::{HttpBuilder, HttpConfig},
    app::run_main,
    client::RegistryConfig,
    defaults,
    health::HealthServerConfig,
    openid::{Authenticator, AuthenticatorConfig},
    openid_auth,
};
use futures::FutureExt;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    pub oauth: AuthenticatorConfig,

    pub service: PostgresServiceConfiguration,

    pub instance: String,
    #[serde(default = "defaults::check_kafka_topic_ready")]
    pub check_kafka_topic_ready: bool,
    pub kafka_downstream_config: KafkaClientConfig,
    #[serde(default)]
    pub endpoint_pool: ExternalClientPoolConfig,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub http: HttpConfig,
}

#[macro_export]
macro_rules! app {
    ($cfg:expr, $data:expr, $auth: expr) => {{
        use $crate::endpoints;

        $cfg.app_data($data.clone()).service(
            web::scope("/api/state/v1alpha1")
                .wrap($auth)
                .service(
                    web::resource("/states/{application}/{device}")
                        .route(web::get().to(endpoints::get)),
                )
                .service(web::resource("/sessions").route(web::put().to(endpoints::init)))
                .service(
                    web::resource("/sessions/{session}").route(web::post().to(endpoints::ping)),
                )
                .service(
                    web::resource("/sessions/{session}/states/{application}/{device}")
                        .route(web::put().to(endpoints::create))
                        .route(web::delete().to(endpoints::delete)),
                ),
        )
    }};
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::info!("Running device state service!");

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    log::info!("Authenticator: {authenticator:?}");
    let authenticator = authenticator.map(web::Data::new);

    // set up registry client
    let registry = config.registry.into_client().await?;

    let mut checks = Vec::<Box<dyn HealthChecked>>::new();

    // downstream sender

    let sender = DownstreamSender::new(
        KafkaSink::from_config(
            config.kafka_downstream_config,
            config.check_kafka_topic_ready,
        )?,
        config.instance,
        config.endpoint_pool,
    )?;

    // service

    let service =
        service::postgres::PostgresDeviceStateService::new(config.service, sender, registry)?;
    checks.push(Box::new(service.clone()));

    let pruner = service::postgres::run_pruner(service.clone()).boxed_local();

    let service: Arc<dyn DeviceStateService> = Arc::new(service);
    let service: web::Data<dyn DeviceStateService> = web::Data::from(service);

    // monitoring

    // main server

    let main = HttpBuilder::new(config.http, move |cfg| {
        let auth = openid_auth!(req -> {
            req
                .app_data::<web::Data<Authenticator>>().as_ref().map(|s|s.get_ref())
        });
        let mut app = app!(cfg, service, auth);

        if let Some(auth) = &authenticator {
            app = app.app_data(auth.clone())
        }

        app.app_data(service.clone());
    })
    .run()?;

    // run

    run_main([main, pruner], config.health, checks).await?;

    // exiting

    Ok(())
}
