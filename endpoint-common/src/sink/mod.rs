mod http;
mod kafka;

pub use self::http::HttpSink;
pub use kafka::*;

use crate::sender::PublishOutcome;
use async_trait::async_trait;
use cloudevents::Event;
use drogue_client::registry;
use thiserror::Error;

pub enum SinkTarget<'a> {
    Events(&'a registry::v1::Application),
    Commands(&'a registry::v1::Application),
}

#[async_trait]
pub trait Sink: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + 'static;

    #[allow(clippy::needless_lifetimes)]
    /// Publish an event.
    async fn publish<'a>(
        &self,
        target: SinkTarget<'a>,
        event: Event,
    ) -> Result<PublishOutcome, SinkError<Self::Error>>;
}

#[derive(Error, Debug)]
pub enum SinkError<E: std::error::Error + 'static> {
    #[error("Build event error")]
    Build(#[from] cloudevents::event::EventBuilderError),
    #[error("Event error")]
    Event(#[from] cloudevents::message::Error),
    #[error("Transport error")]
    Transport(#[source] E),
    #[error("Target error")]
    Target(#[source] Box<dyn std::error::Error + Send>),
}
