mod event;

pub use drogue_cloud_event_common::stream::{EventStream, EventStreamError};
pub use event::*;

use crate::{Event, EventError, KafkaClientConfig};
use anyhow::bail;
use async_trait::async_trait;
use drogue_cloud_event_common::stream::{CustomAck, EventStreamConfig, Handle};
use drogue_cloud_service_api::health::{HealthCheckError, HealthChecked};
use futures::{Stream, StreamExt};
use rdkafka::error::KafkaError;
use serde::Deserialize;
use std::{
    convert::TryInto,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use thiserror::Error;
use tokio::task::JoinHandle;

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
    pub client: KafkaClientConfig,
    pub topic: String,
    pub consumer_group: String,
}

impl From<KafkaStreamConfig> for EventStreamConfig {
    fn from(cfg: KafkaStreamConfig) -> Self {
        let mut properties = cfg.client.custom;
        properties.insert("auto.offset.reset".into(), "earliest".into());

        Self {
            bootstrap_servers: cfg.client.bootstrap_servers,
            properties,
            topic: cfg.topic,
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
    pub fn run<H>(self, handler: H) -> Runner
    where
        H: EventHandler<Event = Event> + Send + Sync + 'static,
    {
        Runner::new(self, handler)
    }
}

pub struct Runner {
    _handle: JoinHandle<Result<(), anyhow::Error>>,
    running: Arc<AtomicBool>,
}

#[async_trait]
impl HealthChecked for Runner {
    async fn is_alive(&self) -> Result<(), HealthCheckError> {
        if !self.running.load(Ordering::Relaxed) {
            HealthCheckError::nok("Event loop not running")
        } else {
            Ok(())
        }
    }
}

impl Runner {
    pub fn new<H>(mut stream: KafkaEventStream<'static>, handler: H) -> Self
    where
        H: EventHandler<Event = Event> + Send + Sync + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        let handle = tokio::spawn(async move {
            while let Some(Ok(event)) = stream.next().await {
                log::debug!("Processing event: {:?}", event);
                let mut cnt = 0;
                while handler.handle(event.deref()).await.is_err() {
                    if cnt > 10 {
                        bail!("Failed to process event");
                    } else {
                        cnt += 1;
                    }
                }
                stream.ack(event)?;
            }
            r.store(false, Ordering::Relaxed);
            Ok::<_, anyhow::Error>(())
        });
        Self {
            _handle: handle,
            running,
        }
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
        let next = self.0.poll_next_unpin(cx);

        log::debug!("Event: {:?}", next);

        match next {
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
