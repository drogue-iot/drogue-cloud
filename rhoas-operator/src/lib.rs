mod controller;

use crate::controller::{app::ApplicationController, ControllerConfig};
use async_std::sync::{Arc, Mutex};
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
use std::fmt::Debug;

use rhoas_kafka_instance_sdk::apis::configuration::Configuration as InstanceConfiguration;
use rhoas_kafka_management_sdk::apis::configuration::Configuration as MgmtConfiguration;

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

    let controller = Arc::new(Mutex::new(BaseController::new(
        config.work_queue,
        "app",
        ApplicationController::new(config.controller, registry).await?,
    )?));

    // event source - device registry

    let registry_dispatcher =
        EventDispatcher::one(FnEventProcessor::new(controller.clone(), is_relevant));
    let registry = KafkaEventStream::new(config.kafka_source)?;
    let registry = registry.run(registry_dispatcher);

    // run

    log::info!("Running service ...");
    if let Some(health) = config.health {
        let health =
            HealthServer::new(health, vec![], Some(prometheus::default_registry().clone()));
        select! {
            _ = health.run().fuse() => (),
            _ = registry.fuse() => (),
        }
    } else {
        futures::try_join!(registry,)?;
    }

    // exiting
    log::info!("Exiting main!");
    Ok(())
}
