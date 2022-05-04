mod controller;
mod kafka;

use crate::controller::{app::ApplicationController, ControllerConfig};
use async_std::sync::{Arc, Mutex};
use drogue_cloud_operator_common::controller::base::{
    queue::WorkQueueConfig, BaseController, EventDispatcher, FnEventProcessor,
};
use drogue_cloud_registry_events::{
    stream::{KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_common::{
    app::run_main, client::RegistryConfig, defaults, health::HealthServerConfig,
};
use futures::{FutureExt, TryFutureExt};
use rdkafka::ClientConfig;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub controller: ControllerConfig,

    pub work_queue: WorkQueueConfig,

    /// The source of change events
    pub kafka_source: KafkaStreamConfig,

    /// The kafka client for creating topics
    #[serde(default)]
    pub kafka_admin: KafkaClientConfig,
}

fn is_relevant(event: &Event) -> Option<String> {
    match event {
        Event::Application {
            path, application, ..
        } if
        // watch the creation of a new application
        path == "." ||
            // watch the finalizer addition
            path == ".metadata" => Some(application.clone()),

        _ => None,
    }
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::debug!("Config: {:#?}", config);

    // client

    let registry = config.registry.into_client().await?;

    // controller

    let client_config: ClientConfig = config.kafka_admin.into();
    let controller = Arc::new(Mutex::new(BaseController::new(
        config.work_queue,
        "app",
        ApplicationController::new(config.controller, registry, client_config.create()?),
    )?));

    // event source - device registry

    let registry_dispatcher =
        EventDispatcher::one(FnEventProcessor::new(controller.clone(), is_relevant));
    let registry = KafkaEventStream::new(config.kafka_source)?;
    let registry = registry.run(registry_dispatcher);

    // run

    log::info!("Running service ...");
    let main = registry.err_into().boxed_local();
    run_main([main], config.health, vec![]).await?;

    // exiting

    log::info!("Exiting main!");
    Ok(())
}
