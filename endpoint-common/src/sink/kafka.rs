use super::*;
use anyhow::Context;
use async_trait::async_trait;
use cloudevents::{
    binding::rdkafka::{FutureRecordExt, MessageRecord},
    event::ExtensionValue,
    AttributesReader,
};
use drogue_cloud_event_common::config::KafkaClientConfig;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    kafka::{KafkaConfigExt, KafkaEventType, KafkaTarget},
};
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
    internal_producer: FutureProducer,
}

impl KafkaSink {
    /// Create a new Kafka sink from a configuration specified by the prefix.
    pub fn new(prefix: &str) -> anyhow::Result<Self> {
        let config = KafkaClientConfig::from_env_prefix(prefix)
            .with_context(|| format!("Failed to parse {} config", prefix))?;

        let kafka_config: ClientConfig = config.into();

        Ok(Self {
            internal_producer: kafka_config.create()?,
        })
    }

    fn create_producer(config: KafkaClientConfig) -> Result<FutureProducer, KafkaError> {
        let config: ClientConfig = config.into();
        config.create()
    }

    async fn send_with(
        producer: &FutureProducer,
        topic: String,
        key: String,
        message_record: MessageRecord,
    ) -> Result<PublishOutcome, SinkError<KafkaSinkError>> {
        let record = FutureRecord::<String, Vec<u8>>::to(&topic)
            .key(&key)
            .message_record(&message_record);

        match producer.send_result(record) {
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

#[async_trait]
impl Sink for KafkaSink {
    type Error = KafkaSinkError;

    #[allow(clippy::needless_lifetimes)]
    async fn publish<'a>(
        &self,
        target: SinkTarget<'a>,
        event: Event,
    ) -> Result<PublishOutcome, SinkError<Self::Error>> {
        let kafka = match target {
            SinkTarget::Commands(app) => app.kafka_target(KafkaEventType::Commands),
            SinkTarget::Events(app) => app.kafka_target(KafkaEventType::Events),
        }
        .map_err(|err| SinkError::Target(Box::new(err)))?;

        let key = match event.extension(crate::EXT_PARTITIONKEY) {
            Some(ExtensionValue::String(key)) => key,
            _ => event.id(),
        }
        .into();

        log::debug!("Key: {}, Kafka Config: {:?}", key, kafka);

        let message_record = MessageRecord::from_event(event)?;

        match kafka {
            KafkaTarget::Internal { topic } => {
                Self::send_with(&self.internal_producer, topic, key, message_record).await
            }
            KafkaTarget::External { config } => {
                let topic = config.topic;
                match Self::create_producer(config.client) {
                    Ok(producer) => Self::send_with(&producer, topic, key, message_record).await,
                    Err(err) => {
                        return Err(SinkError::Transport(KafkaSinkError::Kafka(err)));
                    }
                }
            }
        }
    }
}
