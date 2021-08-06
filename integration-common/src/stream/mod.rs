#[cfg(feature = "with_actix")]
mod actix;

#[cfg(feature = "with_actix")]
pub use self::actix::*;

use cloudevents::Event;
use drogue_cloud_event_common::stream::{self, EventStreamError};
use drogue_cloud_service_api::kafka::KafkaConfig;
use futures::Stream;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug)]
pub struct EventStreamConfig {
    pub kafka: KafkaConfig,
    pub consumer_group: Option<String>,
}

#[derive(Debug)]
pub struct EventStream<'s> {
    stream: stream::EventStream<'s>,
}

impl<'s> EventStream<'s> {
    pub fn new(cfg: EventStreamConfig) -> Result<Self, EventStreamError> {
        let stream = stream::EventStream::new(stream::EventStreamConfig {
            kafka: cfg.kafka,
            consumer_group: cfg.consumer_group,
        })?;

        Ok(Self { stream })
    }
}

impl<'s> From<EventStream<'s>> for stream::EventStream<'s> {
    fn from(s: EventStream<'s>) -> Self {
        s.stream
    }
}

impl<'s> Deref for EventStream<'s> {
    type Target = dyn Stream<Item = Result<Event, EventStreamError>> + Unpin + 's;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for EventStream<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}
