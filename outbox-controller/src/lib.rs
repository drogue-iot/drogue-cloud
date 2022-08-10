mod resend;
mod service;

use crate::{resend::Resender, service::OutboxServiceConfig};
use anyhow::Context;
use async_trait::async_trait;
use drogue_cloud_registry_events::{
    sender::{KafkaEventSender, KafkaSenderConfig},
    stream::{EventHandler, KafkaEventStream, KafkaStreamConfig},
    Event,
};
use drogue_cloud_service_common::{
    app::{Startup, StartupExt},
    config::ConfigFromEnv,
    defaults,
};
use lazy_static::lazy_static;
use prometheus::{register_int_counter, IntCounter};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

lazy_static! {
    static ref MARK_SEEN: IntCounter = register_int_counter!(
        "drogue_registry_events_mark_seen",
        "Events which have been mark as seen",
    )
    .unwrap();
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
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
        MARK_SEEN.inc();
        self.0.mark_seen(event.clone()).await?;
        Ok(())
    }
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
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
    .start(startup);

    // event source

    let source = KafkaEventStream::new(config.kafka_source)?;
    let source = source.run(OutboxHandler(service));

    // run

    startup.spawn(source);

    // exiting

    Ok(())
}
