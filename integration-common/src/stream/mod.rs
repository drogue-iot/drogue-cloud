#[cfg(feature = "with_actix")]
mod actix;
mod error;

#[cfg(feature = "with_actix")]
pub use self::actix::*;
pub use error::*;

use cloudevents::{event::ExtensionValue, Event};
use cloudevents_sdk_rdkafka::MessageExt;
use drogue_cloud_service_api::EXT_APPLICATION;
use futures::{
    task::{Context, Poll},
    Stream, StreamExt,
};
use owning_ref::OwningHandle;
use rdkafka::error::KafkaResult;
use rdkafka::{
    config::{ClientConfig, RDKafkaLogLevel},
    consumer::{stream_consumer::StreamConsumer, CommitMode, Consumer, DefaultConsumerContext},
    message::BorrowedMessage,
    util::Timeout,
    TopicPartitionList,
};
use std::{
    fmt::{Debug, Formatter},
    pin::Pin,
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct EventStreamConfig {
    pub bootstrap_servers: String,
    pub topic: String,
    pub app: String,
    pub consumer_group: Option<String>,
}

pub struct EventStream {
    upstream: OwningHandle<
        Box<StreamConsumer>,
        Box<rdkafka::consumer::MessageStream<'static, DefaultConsumerContext>>,
    >,
    app: String,
}

impl Debug for EventStream {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EventStream")
            .field("app", &self.app)
            .finish()
    }
}

impl EventStream {
    pub fn new(cfg: EventStreamConfig) -> Result<Self, EventStreamError> {
        match &cfg.consumer_group {
            Some(consumer_group) => Self::new_with_group(&cfg, consumer_group.clone()),
            None => {
                // create a random subscriber ID until we can use `new_without_group`.
                let group_id = format!("anonymous.{}", Uuid::new_v4());
                Self::new_with_group(&cfg, group_id)
            }
        }
    }

    /// Create a new message spy without using group management
    ///
    /// This is currently blocked by: https://github.com/edenhill/librdkafka/issues/3261
    #[allow(dead_code)]
    fn new_without_group(cfg: &EventStreamConfig) -> Result<Self, EventStreamError> {
        let consumer: StreamConsumer<DefaultConsumerContext> = ClientConfig::new()
            .set("bootstrap.servers", &cfg.bootstrap_servers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "false")
            .set_log_level(RDKafkaLogLevel::Debug)
            .create()?;

        log::debug!("Created consumer");

        let topic = cfg.topic.clone();

        let metadata =
            consumer.fetch_metadata(Some(&topic), Timeout::After(Duration::from_secs(10)))?;

        let partitions = metadata
            .topics()
            .iter()
            .find(|t| t.name() == topic)
            .map(|topic| topic.partitions())
            .ok_or_else(|| {
                log::debug!("Failed to find metadata for topic");
                EventStreamError::MissingMetadata
            })?;

        log::debug!("Topic has {} partitions", partitions.len());

        let mut assignment = TopicPartitionList::with_capacity(partitions.len());
        for part in partitions {
            log::debug!("Adding partition: {}", part.id());
            assignment.add_partition(&topic, part.id());
        }

        consumer.assign(&assignment)?;

        log::debug!("Subscribed");

        Ok(Self::wrap(cfg.app.clone(), consumer))
    }

    fn new_with_group(cfg: &EventStreamConfig, group_id: String) -> Result<Self, EventStreamError> {
        let consumer: StreamConsumer<DefaultConsumerContext> = ClientConfig::new()
            .set("group.id", &group_id)
            .set("bootstrap.servers", &cfg.bootstrap_servers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set_log_level(RDKafkaLogLevel::Debug)
            .create()?;

        log::debug!("Created consumer");

        consumer.subscribe(&[&cfg.topic])?;

        log::debug!("Subscribed");

        Ok(Self::wrap(cfg.app.clone(), consumer))
    }

    fn wrap(app: String, consumer: StreamConsumer) -> Self {
        Self {
            upstream: OwningHandle::new_with_fn(Box::new(consumer), |c| {
                Box::new(unsafe { &*c }.stream())
            }),
            app,
        }
    }

    /// Test if the message/event matches an optional filter.
    fn matches(&self, event: &Event) -> bool {
        match event.extension(EXT_APPLICATION) {
            Some(ExtensionValue::String(other_app)) => &self.app == other_app,
            _ => false,
        }
    }

    fn ack(&self, msg: &BorrowedMessage) -> KafkaResult<()> {
        self.upstream
            .as_owner()
            .commit_message(&msg, CommitMode::Async)
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        log::debug!("Stream dropped: {:?}", self);
    }
}

impl Stream for EventStream {
    type Item = Result<Event, EventStreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next = self.upstream.poll_next_unpin(cx);

        match next {
            Poll::Pending => Poll::Pending,
            Poll::Ready(next) => match next {
                None => Poll::Ready(None),
                Some(Err(e)) => Poll::Ready(Some(Err(e.into()))),
                Some(Ok(msg)) => {
                    self.ack(&msg)?;

                    let event = msg.to_event()?;

                    match self.matches(&event) {
                        true => Poll::Ready(Some(Ok(event))),
                        false => {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                    }
                }
            },
        }
    }
}
