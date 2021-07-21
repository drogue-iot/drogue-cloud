mod controller;
mod data;
mod ttn;
mod utils;

use crate::controller::{app::ApplicationController, device::DeviceController};
use actix_web::{
    get, middleware,
    web::{self},
    App, HttpResponse, HttpServer, Responder,
};
use anyhow::anyhow;
use async_std::sync::Mutex;
use dotenv::dotenv;
use drogue_client::registry;
use drogue_cloud_operator_common::controller::base::queue::WorkQueueConfig;
use drogue_cloud_operator_common::controller::base::{
    events, BaseController, EventSource, FnEventProcessor,
};
use drogue_cloud_registry_events::Event;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    endpoints::create_endpoint_source,
    health::{HealthServer, HealthServerConfig},
    openid::TokenConfig,
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default)]
    pub registry: RegistryConfig,

    #[serde(default)]
    pub health: HealthServerConfig,

    pub work_queue: WorkQueueConfig,
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

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

fn is_app_relevant(event: &Event) -> Option<String> {
    match event {
        Event::Application {
            path, application, ..
        } if path == "." || path == ".metadata" || path == ".spec.ttn" => Some(application.clone()),
        _ => None,
    }
}

fn is_device_relevant(event: &Event) -> Option<(String, String)> {
    match event {
        Event::Device {
            path,
            application,
            device,
            ..
        } if path == "."
            || path == ".metadata"
            || path == ".spec.ttn"
            || path == ".spec.gatewaySelector" =>
        {
            Some((application.clone(), device.clone()))
        }
        _ => None,
    }
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let endpoint_source = create_endpoint_source()?;
    let endpoints = endpoint_source.eval_endpoints().await?;

    let endpoint_url = endpoints
        .http
        .map(|http| http.url)
        .ok_or_else(|| anyhow!("Missing HTTP endpoint information"))
        .and_then(|url| Ok(Url::parse(&url)?))?
        .join("/ttn/v3")?;

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

    let app_processor = BaseController::new(
        config.work_queue.clone(),
        "app",
        ApplicationController::new(
            registry.clone(),
            ttn::Client::new(client.clone()),
            endpoint_url,
        ),
    )?;
    let device_processor = BaseController::new(
        config.work_queue,
        "device",
        DeviceController::new(registry, ttn::Client::new(client)),
    )?;

    let controller = EventSource::new(vec![
        Box::new(FnEventProcessor::new(
            Mutex::new(device_processor),
            is_device_relevant,
        )),
        Box::new(FnEventProcessor::new(
            Mutex::new(app_processor),
            is_app_relevant,
        )),
    ]);

    let data = web::Data::new(controller);

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main

    let main = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(data.clone())
            .service(index)
            .service(events)
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
