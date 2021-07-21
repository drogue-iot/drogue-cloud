mod controller;
mod data;

use crate::controller::{app::ApplicationController, ControllerConfig};
use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};
use anyhow::Context;
use async_std::sync::Mutex;
use dotenv::dotenv;
use drogue_client::registry;
use drogue_cloud_operator_common::controller::base::queue::WorkQueueConfig;
use drogue_cloud_operator_common::controller::base::{
    events, BaseController, EventSource, FnEventProcessor,
};
use drogue_cloud_registry_events::Event;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::TokenConfig,
};
use futures::TryFutureExt;
use kube::{api::GroupVersionKind, core::DynamicObject, discovery, Api};
use serde::Deserialize;
use serde_json::json;
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

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
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

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let kube = kube::client::Client::try_default()
        .await
        .context("Failed to create Kubernetes client")?;

    // TODO: discover version too
    let gvk = GroupVersionKind::gvk("kafka.strimzi.io", "v1beta2", "KafkaTopic");
    let (kafka_topic_resource, _caps) = discovery::pinned_kind(&kube, &gvk).await?;
    let kafka_topics = Api::<DynamicObject>::namespaced_with(
        kube,
        &config.controller.topic_namespace,
        &kafka_topic_resource,
    );

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

    // app data

    let data = web::Data::new(controller);

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main

    let main = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(data.clone())
            .service(index)
            .service(events)
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
