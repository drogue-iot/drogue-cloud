#[cfg(feature = "with_actix")]
mod actix;

#[cfg(feature = "with_actix")]
pub use self::actix::*;

use cloudevents::Event;
use drogue_cloud_event_common::stream::{self, EventStreamError};
use drogue_cloud_service_api::events::EventTarget;
use drogue_cloud_service_common::kafka::make_topic_resource_name;
use futures::Stream;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug)]
pub struct EventStreamConfig {
    pub bootstrap_servers: String,
    pub properties: HashMap<String, String>,
    pub target: EventTarget,
    pub consumer_group: Option<String>,
}

#[derive(Debug)]
pub struct EventStream {
    stream: stream::EventStream,
}

impl EventStream {
    pub fn new(cfg: EventStreamConfig) -> Result<Self, EventStreamError> {
        let topic = make_topic_resource_name(cfg.target);

        let stream = stream::EventStream::new(stream::EventStreamConfig {
            bootstrap_servers: cfg.bootstrap_servers,
            properties: cfg.properties,
            consumer_group: cfg.consumer_group,
            topic,
        })?;

        Ok(Self { stream })
    }
}

impl From<EventStream> for stream::EventStream {
    fn from(s: EventStream) -> Self {
        s.stream
    }
}

impl Deref for EventStream {
    type Target = dyn Stream<Item = Result<Event, EventStreamError>> + Unpin;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for EventStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}
