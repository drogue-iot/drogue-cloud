use super::*;

use async_trait::async_trait;
use cloudevents::rdkafka::{FutureRecordExt, MessageRecord};
use cloudevents::{event::ExtensionValue, AttributesReader};
use drogue_cloud_service_common::{config::ConfigFromEnv, defaults};
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

#[derive(Clone, Debug, Deserialize)]
pub struct KafkaSinkConfig {
    #[serde(default = "defaults::kafka_bootstrap_servers")]
    pub bootstrap_servers: String,
    pub topic: String,
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

#[derive(Clone)]
pub struct KafkaSink {
    producer: FutureProducer,
    topic: String,
}

impl KafkaSink {
    /// Create a new Kafka sink from a configuration specified by the prefix.
    pub fn new(prefix: &str) -> anyhow::Result<Self> {
        let config = KafkaSinkConfig::from_env_prefix(prefix)
            .with_context(|| format!("Failed to parse {} config", prefix))?;

        let mut kafka_config = ClientConfig::new();
        kafka_config.set("bootstrap.servers", &config.bootstrap_servers);

        for (k, v) in config.custom {
            let k = k.replace('_', ".".into());
            log::debug!("Kafka Option - {} = {}", k, v);
            kafka_config.set(k, v);
        }

        Ok(Self {
            producer: kafka_config.create()?,
            topic: config.topic,
        })
    }
}

#[async_trait]
impl DownstreamSink for KafkaSink {
    type Error = KafkaSinkError;

    async fn publish(&self, event: Event) -> Result<PublishOutcome, DownstreamError<Self::Error>> {
        let key = match event.extension(EXT_PARTITIONKEY) {
            Some(ExtensionValue::String(key)) => key,
            _ => event.id(),
        }
        .into();

        log::debug!("Key: {}", key);

        let message_record = MessageRecord::from_event(event)?;

        let record = FutureRecord::<String, Vec<u8>>::to(&self.topic)
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
                    Err(DownstreamError::Transport(err.into()))
                }
                // producer closed before delivered
                Err(oneshot::Canceled) => {
                    Err(DownstreamError::Transport(KafkaSinkError::Canceled.into()))
                }
            },
            // failed to queue up
            Err((KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull), _)) => {
                Ok(PublishOutcome::QueueFull)
            }
            // some other queue error
            Err((err, _)) => {
                log::debug!("Failed to send: {}", err);
                Err(DownstreamError::Transport(err.into()))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_custom() {
        std::env::set_var("KAFKA__TOPIC", "baz");
        std::env::set_var("KAFKA__CUSTOM__A_B_C", "d.e.f");

        let kafka = KafkaSinkConfig::from_env_prefix("KAFKA").unwrap();

        assert_eq!(kafka.topic, "baz");
        assert_eq!(kafka.custom.get("a_b_c").cloned(), Some("d.e.f".into()));

        std::env::remove_var("KAFKA__TOPIC");
    }
}
