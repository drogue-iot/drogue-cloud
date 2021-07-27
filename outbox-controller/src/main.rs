mod resend;
mod service;

use crate::{resend::Resender, service::OutboxServiceConfig};
use actix::Actor;
use anyhow::Context;
use async_trait::async_trait;
use dotenv::dotenv;
use drogue_cloud_registry_events::{
    sender::{KafkaEventSender, KafkaSenderConfig},
    stream::{EventHandler, KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default = "resend_period")]
    #[serde(with = "humantime_serde")]
    /// Scan every x seconds for resending events.
    pub resend_period: Duration,

    #[serde(default = "before")]
    #[serde(with = "humantime_serde")]
    /// Send events older than x seconds.
    pub before: Duration,

    #[serde(default)]
    pub health: HealthServerConfig,

    pub kafka_sender: KafkaSenderConfig,
    pub kafka_source: KafkaStreamConfig,
}

const fn resend_period() -> Duration {
    Duration::from_secs(60)
}

const fn before() -> Duration {
    Duration::from_secs(5 * 60)
}

struct OutboxHandler(Arc<service::OutboxService>);

#[async_trait]
impl EventHandler for OutboxHandler {
    type Event = Event;
    type Error = anyhow::Error;

    async fn handle(&self, event: &Self::Event) -> Result<(), anyhow::Error> {
        log::debug!("Outbox event: {:?}", event);
        self.0.mark_seen(event.clone()).await?;
        Ok(())
    }
}

#[actix::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env()?;

    let service = Arc::new(service::OutboxService::new(
        OutboxServiceConfig::from_env()?
    )?);

    // create event sender

    let sender = KafkaEventSender::new(config.kafka_sender)
        .context("Unable to create Kafka event sender")?;

    // start resender

    Resender {
        interval: config.resend_period,
        before: chrono::Duration::from_std(config.before)?,
        service: service.clone(),
        sender: Arc::new(sender),
    }
    .start();

    // event source

    let source = KafkaEventStream::new(config.kafka_source)?;
    let source = source.run(OutboxHandler(service));

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // run

    futures::try_join!(health.run(), source)?;

    // exiting

    Ok(())
}
