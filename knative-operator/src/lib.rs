mod controller;

use crate::controller::{app::ApplicationController, ControllerConfig};
use anyhow::Context;
use async_std::sync::{Arc, Mutex};
use drogue_cloud_operator_common::{
    controller::base::{
        queue::WorkQueueConfig, BaseController, EventDispatcher, FnEventProcessor, NameSource,
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
use kube::{api::ListParams, Api};
use kube_runtime::watcher;
use serde::Deserialize;
use std::fmt::Debug;

pub const ANNOTATION_APP_NAME: &str = "drogue.io/application-name";
pub const DEFAULT_IMAGE: &str = "ghcr.io/drogue-iot/knative-event-source:latest";

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

    let kube = kube::client::Client::try_default()
        .await
        .context("Failed to create Kubernetes client")?;

    // k8s resources

    let deployments = Api::namespaced(kube.clone(), &config.controller.target_namespace);

    // client

    let registry = config.registry.into_client().await?;

    // controller

    let controller = Arc::new(Mutex::new(BaseController::new(
        config.work_queue,
        "knative-app",
        ApplicationController::new(config.controller, registry, deployments.clone()),
    )?));

    // event source - device registry

    let registry_dispatcher =
        EventDispatcher::one(FnEventProcessor::new(controller.clone(), is_relevant));
    let registry = KafkaEventStream::new(config.kafka_source)?;
    let registry = registry.run(registry_dispatcher);

    // event source - Deployment

    let watcher_deployments = watcher(deployments, ListParams::default());
    let watcher_deployments =
        watcher_deployments.run_stream(EventDispatcher::one(ResourceProcessor::new(
            controller,
            NameSource::Annotation(ANNOTATION_APP_NAME.into()),
        )));

    // run

    log::info!("Running service ...");
    if let Some(health) = config.health {
        let health =
            HealthServer::new(health, vec![], Some(prometheus::default_registry().clone()));
        futures::try_join!(health.run(), registry, watcher_deployments)?;
    } else {
        futures::try_join!(registry, watcher_deployments)?;
    }

    // exiting

    Ok(())
}
