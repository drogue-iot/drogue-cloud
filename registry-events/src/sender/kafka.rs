use crate::{Event, EventSender, EventSenderError, SenderResult, EXT_PARTITIONKEY};
use async_trait::async_trait;
use cloudevents::{
    binding::rdkafka::{FutureRecordExt, MessageRecord},
    event::ExtensionValue,
    AttributesReader,
};
use drogue_cloud_service_api::kafka::KafkaConfig;
use rdkafka::{
    error::KafkaError,
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
    ClientConfig,
};
use serde::Deserialize;
use std::{convert::TryInto, time::Duration};
use thiserror::Error;
use tracing::instrument;

#[derive(Clone, Debug, Deserialize)]
pub struct KafkaSenderConfig {
    #[serde(flatten)]
    pub client: KafkaConfig,
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub queue_timeout: Option<Duration>,
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
    pub fn new(config: KafkaSenderConfig) -> anyhow::Result<Self> {
        let client_config: ClientConfig = config.client.client.into();

        let queue_timeout = match config.queue_timeout {
            Some(duration) => Timeout::After(duration),
            None => Timeout::Never,
        };

        Ok(Self {
            producer: client_config.create()?,
            topic: config.client.topic,
            queue_timeout,
        })
    }
}

#[async_trait]
impl EventSender for KafkaEventSender {
    type Error = KafkaSenderError;

    #[instrument(
        skip(self,events),
        fields(
            self.topic=%self.topic,
            self.queue_timeout=?self.queue_timeout,
        )
    )]
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
