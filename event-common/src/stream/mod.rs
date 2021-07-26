mod error;

pub use error::*;

use cloudevents::{binding::rdkafka::MessageExt, AttributesReader, AttributesWriter, Data, Event};
use futures::{
    task::{Context, Poll},
    Stream, StreamExt,
};
use owning_ref::OwningHandle;
use rdkafka::{
    config::{ClientConfig, RDKafkaLogLevel},
    consumer::{stream_consumer::StreamConsumer, CommitMode, Consumer, DefaultConsumerContext},
    error::KafkaResult,
    message::BorrowedMessage,
    util::Timeout,
    TopicPartitionList,
};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    pin::Pin,
    time::Duration,
};
use uuid::Uuid;

pub trait AckMode {
    fn configure(config: &mut ClientConfig);
}

pub struct AutoAck;
pub struct CustomAck;

impl AckMode for AutoAck {
    fn configure(config: &mut ClientConfig) {
        config.set("enable.auto.commit", "true");
    }
}

impl AckMode for CustomAck {
    fn configure(config: &mut ClientConfig) {
        config.set("enable.auto.commit", "false");
    }
}

#[derive(Clone, Debug)]
pub struct EventStreamConfig {
    pub bootstrap_servers: String,
    pub properties: HashMap<String, String>,
    pub topic: String,
    pub consumer_group: Option<String>,
}

pub struct EventStream<'s, Ack = AutoAck>
where
    Ack: AckMode,
{
    _marker: PhantomData<Ack>,
    upstream: OwningHandle<
        Box<StreamConsumer>,
        Box<rdkafka::consumer::MessageStream<'s, DefaultConsumerContext>>,
    >,
    topic: String,
}

impl<Ack> Debug for EventStream<'_, Ack>
where
    Ack: AckMode,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EventStream")
            .field("topic", &self.topic)
            .finish()
    }
}

impl<Ack> EventStream<'_, Ack>
where
    Ack: AckMode,
{
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

    /// Create a new common client config
    fn new_config(cfg: &EventStreamConfig) -> ClientConfig {
        let mut config = ClientConfig::new();

        // start with the defaults

        config
            .set("bootstrap.servers", &cfg.bootstrap_servers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set_log_level(RDKafkaLogLevel::Info);

        // add custom properties

        for (k, v) in &cfg.properties {
            config.set(k.replace('_', "."), v);
        }

        // return result

        config
    }

    /// Create a new message spy without using group management
    ///
    /// This is currently blocked by: https://github.com/edenhill/librdkafka/issues/3261
    #[allow(dead_code)]
    fn new_without_group(cfg: &EventStreamConfig) -> Result<Self, EventStreamError> {
        let mut consumer = Self::new_config(cfg);

        // FIXME: don't use non-group with commits
        Ack::configure(&mut consumer);

        let consumer: StreamConsumer<DefaultConsumerContext> = consumer.create()?;

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

        Ok(Self::wrap(cfg.topic.clone(), consumer))
    }

    fn new_with_group(cfg: &EventStreamConfig, group_id: String) -> Result<Self, EventStreamError> {
        let mut consumer = Self::new_config(cfg);
        consumer.set("group.id", &group_id);

        Ack::configure(&mut consumer);

        let consumer: StreamConsumer<DefaultConsumerContext> = consumer.create()?;

        log::debug!("Created consumer");

        consumer.subscribe(&[&cfg.topic])?;

        log::debug!("Subscribed");

        Ok(Self::wrap(cfg.topic.clone(), consumer))
    }

    fn wrap(topic: String, consumer: StreamConsumer) -> Self {
        Self {
            _marker: PhantomData,
            upstream: OwningHandle::new_with_fn(Box::new(consumer), |c| {
                Box::new(unsafe { &*c }.stream())
            }),
            topic,
        }
    }

    pub fn ack<T>(&self, handle: Handle<'_, T>) -> KafkaResult<()> {
        self.do_ack(&handle.msg)
    }

    fn do_ack(&self, msg: &BorrowedMessage) -> KafkaResult<()> {
        self.upstream
            .as_owner()
            .commit_message(&msg, CommitMode::Async)
    }

    /// Check if the content type indicates a JSON payload
    fn is_json_content_type(content_type: &str) -> bool {
        content_type.starts_with("application/json")
            || content_type.starts_with("text/json")
            || content_type.ends_with("+json")
    }

    /// Try to ensure that the data section is JSON encoded when the content type
    /// indicated a JSON payload.
    ///
    /// This is necessary as e.g. reading from Kafka, the payload will always be binary.
    fn fixup_data_type(mut event: Event) -> Event {
        // Pre-flight check if we need to convert
        match (event.datacontenttype(), event.data()) {
            // There is no content.
            (_, None) => return event,
            // This is already JSON, we don't need to do anything.
            (_, Some(Data::Json(_))) => {
                return event;
            }
            // No content type indication, no need to change anything
            (None, _) => return event,
            // Check if the content type indicates JSON, if not, don't convert
            (Some(content_type), _) if !Self::is_json_content_type(&content_type) => {
                return event;
            }
            _ => {}
        }

        // we know now that the content is indicated as JSON, but currently is not -> do the conversion

        let (content_type, schema_type, data) = match event.take_data() {
            (Some(content_type), schema_type, Some(data)) => {
                (Some(content_type), schema_type, Some(Self::make_json(data)))
            }
            data => data,
        };

        // set the data, content type, and schema type again

        if let Some(data) = data {
            event.set_data_unchecked(data);
        }
        event.set_datacontenttype(content_type);
        event.set_dataschema(schema_type);

        // done

        event
    }

    /// Get JSON from the data section, ignore error, don't do checks if we really need to.
    fn make_json(data: Data) -> Data {
        match data {
            Data::Json(json) => Data::Json(json),
            Data::String(ref str) => serde_json::from_str(&str).map_or_else(|_| data, Data::Json),
            Data::Binary(ref slice) => {
                serde_json::from_slice(&slice).map_or_else(|_| data, Data::Json)
            }
        }
    }
}

impl<'s, Ack> Drop for EventStream<'s, Ack>
where
    Ack: AckMode,
{
    fn drop(&mut self) {
        log::debug!("Stream dropped: {:?}", self);
    }
}

#[derive(Debug)]
pub struct Handle<'s, T> {
    value: T,
    msg: BorrowedMessage<'s>,
}

impl<'s, T> Handle<'s, T> {
    pub fn replace<U>(self, value: U) -> Handle<'s, U> {
        Handle {
            value,
            msg: self.msg,
        }
    }

    pub fn map<U, F>(self, f: F) -> Handle<'s, U>
    where
        F: Fn(T) -> U,
    {
        Handle {
            value: f(self.value),
            msg: self.msg,
        }
    }
}

impl<T> Deref for Handle<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Handle<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<'s> Stream for EventStream<'s, AutoAck> {
    type Item = Result<Event, EventStreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next = self.upstream.poll_next_unpin(cx);

        match next {
            Poll::Pending => Poll::Pending,
            Poll::Ready(next) => match next {
                None => Poll::Ready(None),
                Some(Err(e)) => Poll::Ready(Some(Err(e.into()))),
                Some(Ok(msg)) => {
                    self.do_ack(&msg)?;

                    let event = msg.to_event()?;
                    let event = Self::fixup_data_type(event);

                    Poll::Ready(Some(Ok(event)))
                }
            },
        }
    }
}

impl<'s> Stream for EventStream<'s, CustomAck> {
    type Item = Result<Handle<'s, Event>, EventStreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next = self.upstream.poll_next_unpin(cx);

        match next {
            Poll::Pending => Poll::Pending,
            Poll::Ready(next) => match next {
                None => Poll::Ready(None),
                Some(Err(e)) => Poll::Ready(Some(Err(e.into()))),
                Some(Ok(msg)) => {
                    let event = msg.to_event()?;
                    let event = Self::fixup_data_type(event);

                    let event = Handle { value: event, msg };
                    Poll::Ready(Some(Ok(event)))
                }
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cloudevents::{EventBuilder, EventBuilderV10};
    use serde_json::json;
    use url::Url;

    fn event(ct: Option<&str>, data: Option<Data>) -> Event {
        let mut event = EventBuilderV10::new()
            .subject("foo")
            .id("123")
            .ty("type")
            .source("source")
            .build()
            .expect("Build event");

        event.set_dataschema(Some(Url::parse("https://foo.bar").expect("Parse URL")));
        event.set_datacontenttype(ct);
        if let Some(data) = data {
            event.set_data_unchecked(data);
        }

        event
    }

    #[test]
    fn test_fixup_json() {
        for (content_type, input, output) in [
            // is text, convert to json
            (
                Some("text/json"),
                Some(Data::String(r#"{"foo": "bar"}"#.into())),
                Some(Data::Json(json!({"foo": "bar"}))),
            ),
            // is binary, convert to json
            (
                Some("text/json"),
                Some(Data::Binary(r#"{"foo": "bar"}"#.as_bytes().into())),
                Some(Data::Json(json!({"foo": "bar"}))),
            ),
            // is text, but content type doesn't indicate JSON, leave it
            (
                Some("text/plain"),
                Some(Data::String(r#"{"foo": "bar"}"#.into())),
                Some(Data::String(r#"{"foo": "bar"}"#.into())),
            ),
            // JSON, but no paylod
            (Some("text/json"), None, None),
            // JSON, but broken payload, leave it alone
            (
                Some("text/json"),
                Some(Data::String(r#"{"foo""#.into())),
                Some(Data::String(r#"{"foo""#.into())),
            ),
        ] {
            let event = event(content_type, input);
            let (ct, st, data) = EventStream::<AutoAck>::fixup_data_type(event).take_data();

            assert_eq!(ct.as_deref(), content_type);
            assert_eq!(st.as_ref().map(Url::as_str), Some("https://foo.bar/"));
            assert_eq!(data, output);
        }
    }
}
