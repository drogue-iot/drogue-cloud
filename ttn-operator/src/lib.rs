mod controller;
mod data;
mod ttn;
mod utils;

use crate::controller::{app::ApplicationController, device::DeviceController};
use anyhow::anyhow;
use async_std::sync::Mutex;
use drogue_client::registry;
use drogue_cloud_operator_common::controller::base::{
    queue::WorkQueueConfig, BaseController, EventDispatcher, FnEventProcessor,
};
use drogue_cloud_registry_events::{
    stream::{KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    endpoints::create_endpoint_source,
    health::{HealthServer, HealthServerConfig},
    openid::TokenConfig,
};
use futures::TryFutureExt;
use serde::Deserialize;
use std::sync::Arc;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default)]
    pub registry: RegistryConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub work_queue: WorkQueueConfig,

    pub kafka_source: KafkaStreamConfig,
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

pub async fn run(config: Config) -> anyhow::Result<()> {
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

    let controller = EventDispatcher::new(vec![
        Box::new(FnEventProcessor::new(
            Arc::new(Mutex::new(device_processor)),
            is_device_relevant,
        )),
        Box::new(FnEventProcessor::new(
            Arc::new(Mutex::new(app_processor)),
            is_app_relevant,
        )),
    ]);

    // event source

    let source = KafkaEventStream::new(config.kafka_source)?;
    let source = source.run(controller);

    // run

    log::info!("Running service ...");
    if let Some(health) = config.health {
        let health = HealthServer::new(health, vec![]);
        futures::try_join!(health.run(), source.err_into())?;
    } else {
        futures::try_join!(source)?;
    }

    // exiting

    Ok(())
}
