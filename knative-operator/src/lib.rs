mod controller;

use crate::controller::{app::ApplicationController, ControllerConfig};
use anyhow::Context;
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
    app::{Startup, StartupExt},
    client::ClientConfig,
    defaults,
};
use kube::{api::ListParams, Api};
use kube_runtime::watcher;
use serde::Deserialize;
use std::{fmt::Debug, sync::Arc};
use tokio::sync::Mutex;

pub const LABEL_APP_MARKER: &str = "drogue.io/application";
/// We need an annotation to store the actual Drogue Cloud application name, which is not a valid
/// Kubernetes label value.
pub const ANNOTATION_APP_NAME: &str = "drogue.io/application-name";

pub const DEFAULT_IMAGE: &str = "ghcr.io/drogue-iot/knative-event-source:latest";

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    pub registry: ClientConfig,

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
            path == "." 
            // watch the finalizer addition
            || path == ".metadata"
            // watch the spec section
            || path == ".spec.knative" => Some(application.clone()),

        _ => None,
    }
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
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

    let watcher_deployments = watcher(
        deployments,
        ListParams {
            // only watch deployments having the app name annotation
            label_selector: Some(LABEL_APP_MARKER.to_string()),
            ..Default::default()
        },
    );
    let watcher_deployments =
        watcher_deployments.run_stream(EventDispatcher::one(ResourceProcessor::new(
            controller,
            NameSource::Annotation(ANNOTATION_APP_NAME.into()),
        )));

    // run

    startup.spawn(registry);
    startup.spawn(watcher_deployments);

    // exiting

    Ok(())
}
