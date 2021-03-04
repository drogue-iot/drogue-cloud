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
use drogue_cloud_registry_events::reqwest::ReqwestEventSender;
use drogue_cloud_service_common::config::ConfigFromEnv;
use envconfig::Envconfig;
use serde_json::json;
use std::sync::Arc;
use url::Url;

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,

    #[envconfig(from = "RESEND_PERIOD", default = "1m")]
    /// Scan every x seconds for resending events.
    pub resend_period: humantime::Duration,

    #[envconfig(from = "BEFORE", default = "5m")]
    /// Send events older than x seconds.
    pub resend_before: humantime::Duration,

    #[envconfig(from = "K_SINK")]
    pub event_url: Url,
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

    let config = Config::init_from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let service = service::OutboxService::new(OutboxServiceConfig::from_env()?)?;

    // create event sender
    let sender = ReqwestEventSender::new(
        reqwest::ClientBuilder::new()
            .build()
            .context("Failed to create event sender client")?,
        config.event_url,
    );

    // start resender
    Resender {
        interval: config.resend_period.into(),
        before: chrono::Duration::from_std(config.resend_before.into())?,
        service: Arc::new(service.clone()),
        sender: Arc::new(sender),
    }
    .start();

    let data = web::Data::new(WebData { service });

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(data.clone())
            .service(endpoints::health)
            .service(index)
            .service(endpoints::events)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
