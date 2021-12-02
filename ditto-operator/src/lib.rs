mod controller;
mod ditto;

use crate::controller::{app::ApplicationController, ControllerConfig};
use async_std::sync::Mutex;
use drogue_cloud_operator_common::controller::base::{
    queue::WorkQueueConfig, BaseController, EventDispatcher, FnEventProcessor,
};
use drogue_cloud_registry_events::{
    stream::{KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_common::{
    client::RegistryConfig,
    defaults,
    health::{HealthServer, HealthServerConfig},
};
use futures::{select, FutureExt};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub work_queue: WorkQueueConfig,

    pub kafka_source: KafkaStreamConfig,

    pub controller: ControllerConfig,
}

fn is_app_relevant(event: &Event) -> Option<String> {
    match event {
        Event::Application {
            path, application, ..
        } if path == "." || path == ".metadata" || path == ".spec.ditto" => {
            Some(application.clone())
        }
        _ => None,
    }
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::debug!("Config: {:#?}", config);

    // client

    let client = reqwest::Client::new();
    let registry = config.registry.into_client(client.clone()).await?;

    // controller

    let app_processor = BaseController::new(
        config.work_queue.clone(),
        "app",
        ApplicationController::new(config.controller, registry),
    )?;

    let controller = EventDispatcher::new(vec![Box::new(FnEventProcessor::new(
        Arc::new(Mutex::new(app_processor)),
        is_app_relevant,
    ))]);

    // event source

    let source = KafkaEventStream::new(config.kafka_source)?;
    let source = source.run(controller);

    // run

    log::info!("Running service ...");
    if let Some(health) = config.health {
        let health = HealthServer::new(health, vec![]);
        select! {
            _ = health.run().fuse() => (),
            _ = source.fuse() => (),
        }
    } else {
        source.await?;
    }

    // exiting
    log::info!("Exiting main!");
    Ok(())
}
