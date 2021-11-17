use rdkafka::{
    admin::TopicResult,
    error::{KafkaError, KafkaResult},
};

pub trait TopicErrorConverter {
    fn single_topic_response(self) -> KafkaResult<()>;
}

impl TopicErrorConverter for KafkaResult<Vec<TopicResult>> {
    fn single_topic_response(self) -> KafkaResult<()> {
        self.and_then(|mut r| match r.pop() {
            Some(Ok(_)) => Ok(()),
            Some(Err((_, err))) => Err(KafkaError::AdminOp(err)),
            None => Err(KafkaError::AdminOpCreation("Missing response".into())),
        })
    }
}
