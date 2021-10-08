mod controller;

use crate::controller::{
    app::{ApplicationController, ANNOTATION_APP_NAME},
    ControllerConfig,
};
use anyhow::{anyhow, Context};
use async_std::sync::{Arc, Mutex};
use drogue_cloud_operator_common::{
    controller::base::{
        queue::WorkQueueConfig, BaseController, EventDispatcher, FnEventProcessor,
        ResourceProcessor,
    },
    watcher::RunStream,
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
use k8s_openapi::api::core::v1::Secret;
use kube::{api::ListParams, core::DynamicObject, discovery, Api};
use kube_runtime::watcher;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default)]
    pub registry: Option<RegistryConfig>,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub controller: ControllerConfig,

    pub work_queue: WorkQueueConfig,

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

const GROUP_KAFKA_STRIMZI_IO: &str = "kafka.strimzi.io";
const KIND_KAFKA_TOPIC: &str = "KafkaTopic";
const KIND_KAFKA_USER: &str = "KafkaUser";

pub async fn run(config: Config) -> anyhow::Result<()> {
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
    let (kafka_user_resource, _caps) = group
        .recommended_kind(KIND_KAFKA_USER)
        .ok_or_else(|| anyhow!("Unable to discover '{}'", KIND_KAFKA_USER))?;
    let kafka_users = Api::<DynamicObject>::namespaced_with(
        kube.clone(),
        &config.controller.topic_namespace,
        &kafka_user_resource,
    );
    let secrets = Api::<Secret>::namespaced(kube.clone(), &config.controller.topic_namespace);

    // client

    let client = reqwest::Client::new();
    let registry = config
        .registry
        .context("no registry configured")?
        .into_client(client.clone())
        .await?;

    // controller

    let controller = Arc::new(Mutex::new(BaseController::new(
        config.work_queue,
        "app",
        ApplicationController::new(
            config.controller,
            registry,
            kafka_topic_resource,
            kafka_topics.clone(),
            kafka_user_resource,
            kafka_users.clone(),
            secrets.clone(),
        ),
    )?));

    // event source - device registry

    let registry_dispatcher =
        EventDispatcher::one(FnEventProcessor::new(controller.clone(), is_relevant));
    let registry = KafkaEventStream::new(config.kafka_source)?;
    let registry = registry.run(registry_dispatcher);

    // event source - KafkaTopic

    let watcher_topics = watcher(kafka_topics, ListParams::default());
    let watcher_topics = watcher_topics.run_stream(EventDispatcher::one(ResourceProcessor::new(
        controller.clone(),
        ANNOTATION_APP_NAME,
    )));

    // event source - KafkaUser

    let watcher_users = watcher(kafka_users, ListParams::default());
    let watcher_users = watcher_users.run_stream(EventDispatcher::one(ResourceProcessor::new(
        controller.clone(),
        ANNOTATION_APP_NAME,
    )));

    // event source - Secret

    let watcher_secret = watcher(secrets, ListParams::default());
    let watcher_secret = watcher_secret.run_stream(EventDispatcher::one(ResourceProcessor::new(
        controller,
        ANNOTATION_APP_NAME,
    )));

    // run

    log::info!("Running service ...");
    if let Some(health) = config.health {
        let health = HealthServer::new(health, vec![]);
        futures::try_join!(
            health.run(),
            registry,
            watcher_topics,
            watcher_users,
            watcher_secret
        )?;
    } else {
        futures::try_join!(registry, watcher_topics, watcher_users, watcher_secret)?;
    }

    // exiting

    Ok(())
}
