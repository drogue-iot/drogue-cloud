use crate::{Event, EventSender, EventSenderError, SenderResult, EXT_PARTITIONKEY};
use anyhow::Context;
use async_trait::async_trait;
use cloudevents::rdkafka::{FutureRecordExt, MessageRecord};
use cloudevents::{event::ExtensionValue, AttributesReader};
use drogue_cloud_service_common::{config::ConfigFromEnv, defaults};
use rdkafka::{
    error::KafkaError,
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
    ClientConfig,
};
use serde::Deserialize;
use std::{collections::HashMap, convert::TryInto, time::Duration};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize)]
pub struct KafkaSenderConfig {
    #[serde(default = "defaults::kafka_bootstrap_servers")]
    pub bootstrap_servers: String,
    pub topic: String,
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub queue_timeout: Option<Duration>,
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

#[derive(Debug, Error)]
pub enum KafkaSenderError {
    #[error("Kafka error")]
    Kafka(#[from] KafkaError),
    #[error("Transmission canceled")]
    Canceled,
}

#[derive(Clone)]
pub struct KafkaEventSender {
    producer: FutureProducer,
    topic: String,
    queue_timeout: Timeout,
}

impl KafkaEventSender {
    /// Create a new Kafka sender from a configuration specified by the prefix.
    pub fn new(prefix: &str) -> anyhow::Result<Self> {
        let config = KafkaSenderConfig::from_env_prefix(prefix)
            .with_context(|| format!("Failed to parse {} config", prefix))?;

        let mut kafka_config = ClientConfig::new();
        kafka_config.set("bootstrap.servers", &config.bootstrap_servers);

        for (k, v) in config.custom {
            log::info!("Kafka Option - {} = {}", k, v);
            kafka_config.set(k, v);
        }

        let queue_timeout = match config.queue_timeout {
            Some(duration) => Timeout::After(duration),
            None => Timeout::Never,
        };

        Ok(Self {
            producer: kafka_config.create()?,
            topic: config.topic,
            queue_timeout,
        })
    }
}

#[async_trait]
impl EventSender for KafkaEventSender {
    type Error = KafkaSenderError;

    async fn notify<I>(&self, events: I) -> SenderResult<(), Self::Error>
    where
        I: IntoIterator<Item = Event> + Sync + Send,
        I::IntoIter: Sync + Send,
    {
        for event in events.into_iter() {
            let event: cloudevents::Event = event.try_into().map_err(EventSenderError::Event)?;

            let key = match event.extension(EXT_PARTITIONKEY) {
                Some(ExtensionValue::String(key)) => key,
                _ => event.id(),
            }
            .into();

            log::debug!("Key: {}", key);

            let message_record =
                MessageRecord::from_event(event).map_err(EventSenderError::CloudEvent)?;

            let record = FutureRecord::<String, Vec<u8>>::to(&self.topic)
                .key(&key)
                .message_record(&message_record);

            self.producer
                .send(record, self.queue_timeout)
                .await
                .map_err(|(err, _)| KafkaSenderError::Kafka(err))?;
        }

        Ok(())
    }
}
