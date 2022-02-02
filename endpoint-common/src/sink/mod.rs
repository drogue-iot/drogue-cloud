mod http;
mod kafka;

pub use self::http::HttpSink;
pub use kafka::*;
use std::fmt::Debug;

use crate::sender::PublishOutcome;
use async_trait::async_trait;
use cloudevents::Event;
use drogue_client::registry;
use std::ops::Deref;
use thiserror::Error;

#[derive(Debug)]
pub enum SinkTarget<'a> {
    Events(&'a registry::v1::Application),
    Commands(&'a registry::v1::Application),
}

impl<'a> Deref for SinkTarget<'a> {
    type Target = registry::v1::Application;

    fn deref(&self) -> &Self::Target {
        match self {
            SinkTarget::Commands(app) => app,
            SinkTarget::Events(app) => app,
        }
    }
}

#[async_trait]
pub trait Sink: Clone + Send + Sync + Debug + 'static {
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
    #[error("Event error")]
    Event(#[from] cloudevents::message::Error),
    #[error("Transport error")]
    Transport(#[source] E),
    #[error("Target error")]
    Target(#[source] Box<dyn std::error::Error + Send>),
}
