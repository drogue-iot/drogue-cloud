mod controller;
mod data;
mod ditto;

use crate::controller::{app::ApplicationController, device::DeviceController, ControllerConfig};
use async_std::sync::Mutex;
use drogue_cloud_operator_common::controller::base::{
    queue::WorkQueueConfig, BaseController, EventDispatcher, FnEventProcessor,
};
use drogue_cloud_registry_events::{
    stream::{KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_common::{
    app::run_main, client::RegistryConfig, defaults, health::HealthServerConfig,
    reqwest::ClientFactory,
};
use futures::{FutureExt, TryFutureExt};
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

fn is_device_relevant(event: &Event) -> Option<(String, String)> {
    match event {
        Event::Device {
            path,
            application,
            device,
            ..
        } if path == "." || path == ".metadata" || path == ".spec.ditto" => {
            Some((application.clone(), device.clone()))
        }
        _ => None,
    }
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::debug!("Config: {:#?}", config);

    // client

    let client = ClientFactory::new().build()?;
    let registry = config.registry.into_client().await?;

    // controller

    let app_processor = BaseController::new(
        config.work_queue.clone(),
        "app",
        ApplicationController::new(config.controller.clone(), registry.clone(), client.clone())
            .await?,
    )?;

    let device_processor = BaseController::new(
        config.work_queue.clone(),
        "device",
        DeviceController::new(config.controller, registry, client).await?,
    )?;

    let controller = EventDispatcher::new(vec![
        Box::new(FnEventProcessor::new(
            Arc::new(Mutex::new(app_processor)),
            is_app_relevant,
        )),
        Box::new(FnEventProcessor::new(
            Arc::new(Mutex::new(device_processor)),
            is_device_relevant,
        )),
    ]);

    // event source

    let source = KafkaEventStream::new(config.kafka_source)?;
    let source = source.run(controller);

    // run

    log::info!("Running service ...");

    let main = source.err_into().boxed_local();
    run_main([main], config.health, vec![]).await?;

    // exiting
    log::info!("Exiting main!");
    Ok(())
}
