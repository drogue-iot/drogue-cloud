#[cfg(feature = "with_actix")]
mod actix;

#[cfg(feature = "with_actix")]
pub use self::actix::*;

use drogue_cloud_event_common::stream::{self, AckMode, AutoAck, EventStreamError};
use drogue_cloud_service_api::kafka::KafkaConfig;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug)]
pub struct EventStreamConfig {
    pub kafka: KafkaConfig,
    pub consumer_group: Option<String>,
}

#[derive(Debug)]
pub struct EventStream<'s, Ack = AutoAck>
where
    Ack: AckMode,
{
    stream: stream::EventStream<'s, Ack>,
}

impl<'s, Ack> EventStream<'s, Ack>
where
    Ack: AckMode,
{
    pub fn new(cfg: EventStreamConfig) -> Result<Self, EventStreamError> {
        let stream = stream::EventStream::new(stream::EventStreamConfig {
            kafka: cfg.kafka,
            consumer_group: cfg.consumer_group,
        })?;

        Ok(Self { stream })
    }
}

impl<'s, Ack> From<EventStream<'s, Ack>> for stream::EventStream<'s, Ack>
where
    Ack: AckMode,
{
    fn from(s: EventStream<'s, Ack>) -> Self {
        s.stream
    }
}

impl<'s, Ack: AckMode> Deref for EventStream<'s, Ack> {
    type Target = stream::EventStream<'s, Ack>;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl<'s, Ack: AckMode> DerefMut for EventStream<'s, Ack> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}
