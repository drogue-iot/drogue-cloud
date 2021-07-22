use rdkafka::error::KafkaError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventStreamError {
    #[error("Kafka error: {0}")]
    Kafka(#[from] KafkaError),
    #[error("Missing metadata")]
    MissingMetadata,
    #[error("Cloud event error: {0}")]
    CloudEvent(#[from] cloudevents::message::Error),
}
