use super::*;
use anyhow::Context;
use async_trait::async_trait;
use cloudevents::{
    binding::rdkafka::{FutureRecordExt, MessageRecord},
    event::ExtensionValue,
    AttributesReader,
};
use drogue_cloud_event_common::config::KafkaClientConfig;
use drogue_cloud_service_api::events::EventTarget;
use drogue_cloud_service_common::{config::ConfigFromEnv, kafka::make_topic_resource_name};
use futures::channel::oneshot;
use rdkafka::{
    error::{KafkaError, RDKafkaErrorCode},
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KafkaSinkError {
    #[error("Kafka error")]
    Kafka(#[from] KafkaError),
    #[error("Transmission canceled")]
    Canceled,
}

#[derive(Clone)]
pub struct KafkaSink {
    producer: FutureProducer,
}

impl KafkaSink {
    /// Create a new Kafka sink from a configuration specified by the prefix.
    pub fn new(prefix: &str) -> anyhow::Result<Self> {
        let config = KafkaClientConfig::from_env_prefix(prefix)
            .with_context(|| format!("Failed to parse {} config", prefix))?;

        let kafka_config: ClientConfig = config.into();

        Ok(Self {
            producer: kafka_config.create()?,
        })
    }
}

#[async_trait]
impl Sink for KafkaSink {
    type Error = KafkaSinkError;

    #[allow(clippy::needless_lifetimes)]
    async fn publish<'a>(
        &self,
        target: SinkTarget<'a>,
        event: Event,
    ) -> Result<PublishOutcome, SinkError<Self::Error>> {
        let key = match event.extension(crate::EXT_PARTITIONKEY) {
            Some(ExtensionValue::String(key)) => key,
            _ => event.id(),
        }
        .into();

        let topic = make_topic_resource_name(match target {
            SinkTarget::Events(app) => EventTarget::Events(app.metadata.name.clone()),
            SinkTarget::Commands(app) => EventTarget::Commands(app.metadata.name.clone()),
        });

        log::debug!("Key: {}, Topic: {}", key, topic);

        let message_record = MessageRecord::from_event(event)?;

        let record = FutureRecord::<String, Vec<u8>>::to(&topic)
            .key(&key)
            .message_record(&message_record);

        match self.producer.send_result(record) {
            // accepted deliver
            Ok(fut) => match fut.await {
                // received outcome & outcome ok
                Ok(Ok(_)) => Ok(PublishOutcome::Accepted),
                // received outcome & outcome failed
                Ok(Err((err, _))) => {
                    log::debug!("Kafka transport error: {}", err);
                    Err(SinkError::Transport(err.into()))
                }
                // producer closed before delivered
                Err(oneshot::Canceled) => Err(SinkError::Transport(KafkaSinkError::Canceled)),
            },
            // failed to queue up
            Err((KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull), _)) => {
                Ok(PublishOutcome::QueueFull)
            }
            // some other queue error
            Err((err, _)) => {
                log::debug!("Failed to send: {}", err);
                Err(SinkError::Transport(err.into()))
            }
        }
    }
}
