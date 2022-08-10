mod controller;

use crate::controller::{resource::ApplicationAndDeviceKey, ControllerConfig, EventController};
use drogue_cloud_endpoint_common::{
    sender::{DownstreamSender, ExternalClientPoolConfig},
    sink::KafkaSink,
};
use drogue_cloud_operator_common::controller::base::{
    queue::WorkQueueConfig, BaseController, EventDispatcher, FnEventProcessor,
};
use drogue_cloud_registry_events::{
    stream::{KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_common::{
    app::{Startup, StartupExt},
    client::ClientConfig,
    defaults,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    pub registry: ClientConfig,

    #[serde(default)]
    pub controller: ControllerConfig,

    pub work_queue: WorkQueueConfig,

    pub kafka_source: KafkaStreamConfig,

    pub instance: String,
    #[serde(default = "defaults::check_kafka_topic_ready")]
    pub check_kafka_topic_ready: bool,
    pub kafka_downstream_config: KafkaClientConfig,
    #[serde(default)]
    pub endpoint_pool: ExternalClientPoolConfig,
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    // downstream sender

    let sender = DownstreamSender::new(
        KafkaSink::from_config(
            config.kafka_downstream_config,
            config.check_kafka_topic_ready,
        )?,
        config.instance,
        config.endpoint_pool,
    )?;

    // registry client

    let registry = config.registry.into_client().await?;

    // event source

    let controller = Arc::new(Mutex::new(BaseController::new(
        config.work_queue,
        "mgmt-events",
        EventController::new(config.controller, registry, sender),
    )?));

    // event source - device registry

    let registry_dispatcher =
        EventDispatcher::one(FnEventProcessor::new(controller, |evt| match evt {
            Event::Device {
                application,
                uid,
                device,
                ..
            } => Some(ApplicationAndDeviceKey {
                application: application.clone(),
                device: device.clone(),
                device_uid: uid.clone(),
            }),
            _ => None,
        }));
    let registry = KafkaEventStream::new(config.kafka_source)?;
    let registry = registry.run(registry_dispatcher);

    // run

    startup.spawn(registry);

    // exiting

    Ok(())
}
