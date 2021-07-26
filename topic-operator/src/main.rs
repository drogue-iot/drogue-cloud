mod controller;
mod data;

use crate::controller::{app::ApplicationController, ControllerConfig};
use anyhow::{anyhow, Context};
use async_std::sync::Mutex;
use dotenv::dotenv;
use drogue_client::registry;
use drogue_cloud_operator_common::controller::base::{
    queue::WorkQueueConfig, BaseController, EventSource, FnEventProcessor,
};
use drogue_cloud_registry_events::{
    stream::{KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::TokenConfig,
};
use kube::{core::DynamicObject, discovery, Api};
use serde::Deserialize;
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

    pub controller: ControllerConfig,

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

const GROUP_KAFKA_STRIMZI_IO: &str = "kafka.strimzi.io";
const KIND_KAFKA_TOPIC: &str = "KafkaTopic";

#[actix::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;

    let kube = kube::client::Client::try_default()
        .await
        .context("Failed to create Kubernetes client")?;

    // k8s resources

    let group = discovery::group(&kube, GROUP_KAFKA_STRIMZI_IO).await?;
    let (kafka_topic_resource, _caps) = group
        .recommended_kind(KIND_KAFKA_TOPIC)
        .ok_or_else(|| anyhow!("Unable to discover '{}'", KIND_KAFKA_TOPIC))?;
    let kafka_topics = Api::<DynamicObject>::namespaced_with(
        kube.clone(),
        &config.controller.topic_namespace,
        &kafka_topic_resource,
    );

    // client

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

    // controller

    let processor = BaseController::new(
        config.work_queue,
        "app",
        ApplicationController::new(
            config.controller,
            registry,
            kafka_topic_resource,
            kafka_topics,
        ),
    )?;
    let controller = EventSource::one(FnEventProcessor::new(Mutex::new(processor), is_relevant));

    // event source

    let source = KafkaEventStream::new(config.kafka_source)?;
    let source = source.run(controller);

    // health server

    let health = HealthServer::new(config.health, vec![Box::new(source)]);

    // run

    log::info!("Running service ...");
    futures::try_join!(health.run())?;

    // exiting

    Ok(())
}
