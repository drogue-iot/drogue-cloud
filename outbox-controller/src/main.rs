mod endpoints;
mod resend;
mod service;

use crate::resend::Resender;
use crate::service::{OutboxService, OutboxServiceConfig};
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
use std::time::Duration;
use url::Url;

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,

    #[envconfig(from = "RESEND_PERIOD", default = "60")]
    /// Scan every x seconds for resending events.
    pub resend_period: u32,

    #[envconfig(from = "BEFORE", default = "300")]
    /// Send events older than x seconds.
    pub resend_before: u32,

    #[envconfig(from = "K_SINK", default = "300")]
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
        interval: Duration::from_secs(config.resend_period as u64),
        before: chrono::Duration::seconds(config.resend_before as i64),
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
