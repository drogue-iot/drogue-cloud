mod event;

pub use drogue_cloud_event_common::stream::{EventStream, EventStreamError};
pub use event::*;

use crate::{Event, EventError};
use anyhow::bail;
use drogue_cloud_event_common::stream::{CustomAck, EventStreamConfig, Handle};
use drogue_cloud_service_api::kafka::KafkaConfig;
use futures::{Stream, StreamExt, TryStreamExt};
use rdkafka::error::KafkaError;
use serde::Deserialize;
use std::{
    convert::TryInto,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KafkaStreamError {
    #[error("Stream failed: {0}")]
    Stream(#[from] EventStreamError),
    #[error("Event failed: {0}")]
    Event(#[from] EventError),
}

impl From<KafkaError> for KafkaStreamError {
    fn from(err: KafkaError) -> Self {
        Self::Stream(err.into())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct KafkaStreamConfig {
    #[serde(flatten)]
    pub client: KafkaConfig,
    pub consumer_group: String,
}

impl From<KafkaStreamConfig> for EventStreamConfig {
    fn from(mut cfg: KafkaStreamConfig) -> Self {
        let properties = &mut cfg.client.client.properties;
        properties.insert("auto.offset.reset".into(), "earliest".into());

        Self {
            kafka: cfg.client,
            consumer_group: Some(cfg.consumer_group),
        }
    }
}

pub struct KafkaEventStream<'s>(EventStream<'s, CustomAck>);

impl<'s> KafkaEventStream<'s> {
    pub fn new(cfg: KafkaStreamConfig) -> Result<Self, KafkaStreamError> {
        Ok(EventStream::new(cfg.into()).map(Self)?)
    }
}

impl KafkaEventStream<'static> {
    pub async fn run<H>(self, handler: H) -> Result<(), anyhow::Error>
    where
        H: EventHandler<Event = Event> + Send + Sync + 'static,
    {
        let mut stream = self;
        while let Some(event) = stream.try_next().await? {
            log::debug!("Processing event: {:?}", event);
            let mut cnt = 0;
            // try to handle it
            while handler.handle(event.deref()).await.is_err() {
                if cnt > 10 {
                    bail!("Failed to process event");
                } else {
                    cnt += 1;
                }
            }
            // if we had been successful, ack it
            stream.ack(event)?;
        }
        bail!("Stream must not end")
    }
}

impl<'s> Deref for KafkaEventStream<'s> {
    type Target = EventStream<'s, CustomAck>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'s> DerefMut for KafkaEventStream<'s> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'s> Stream for KafkaEventStream<'s> {
    type Item = Result<Handle<'s, Event>, KafkaStreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.0.poll_next_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(next) => match next {
                None => Poll::Ready(None),
                Some(Err(e)) => Poll::Ready(Some(Err(e.into()))),
                Some(Ok(handle)) => {
                    let event: Event = handle.deref().clone().try_into()?;

                    Poll::Ready(Some(Ok(handle.replace(event))))
                }
            },
        }
    }
}
