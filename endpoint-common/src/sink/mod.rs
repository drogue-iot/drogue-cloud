mod http;
mod kafka;

pub use self::http::HttpSink;
pub use kafka::*;

use crate::sender::PublishOutcome;
use async_trait::async_trait;
use cloudevents::Event;
use drogue_client::registry;
use std::{fmt::Debug, ops::Deref};
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
pub trait Sink: Send + Sync + Debug + 'static {
    #[allow(clippy::needless_lifetimes)]
    /// Publish an event.
    async fn publish<'a>(
        &self,
        target: SinkTarget<'a>,
        event: Event,
    ) -> Result<PublishOutcome, SinkError>;
}

#[derive(Error, Debug)]
pub enum SinkError {
    #[error("Event error")]
    Event(#[from] cloudevents::message::Error),
    #[error("Transport error")]
    Transport(#[source] Box<dyn std::error::Error + Send>),
    #[error("Target error")]
    Target(#[source] Box<dyn std::error::Error + Send>),
}
