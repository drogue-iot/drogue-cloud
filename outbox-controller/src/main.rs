mod endpoints;
mod resend;
mod service;

use crate::{
    resend::Resender,
    service::{OutboxService, OutboxServiceConfig},
};
use actix::Actor;
use actix_web::{
    get, middleware,
    web::{self},
    App, HttpResponse, HttpServer, Responder,
};
use anyhow::Context;
use dotenv::dotenv;
use drogue_cloud_registry_events::kafka::KafkaEventSender;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default = "resend_period")]
    #[serde(with = "humantime_serde")]
    /// Scan every x seconds for resending events.
    pub resend_period: Duration,

    #[serde(default = "before")]
    #[serde(with = "humantime_serde")]
    /// Send events older than x seconds.
    pub before: Duration,

    #[serde(default)]
    pub health: HealthServerConfig,
}

const fn resend_period() -> Duration {
    Duration::from_secs(60)
}

const fn before() -> Duration {
    Duration::from_secs(5 * 60)
}

pub struct WebData {
    pub service: OutboxService,
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

    let service = service::OutboxService::new(OutboxServiceConfig::from_env()?)?;

    // create event sender
    let sender =
        KafkaEventSender::new("KAFKA_EVENTS").context("Unable to create Kafka event sender")?;

    // start resender
    Resender {
        interval: config.resend_period.into(),
        before: chrono::Duration::from_std(config.before.into())?,
        service: Arc::new(service.clone()),
        sender: Arc::new(sender),
    }
    .start();

    let data = web::Data::new(WebData { service });

    // health server

    let health = HealthServer::new(config.health, vec![Box::new(data.service.clone())]);

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
