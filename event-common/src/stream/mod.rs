mod error;

pub use error::*;

use cloudevents::{binding::rdkafka::MessageExt, AttributesReader, AttributesWriter, Data, Event};
use drogue_cloud_service_api::kafka::KafkaConfig;
use futures::{
    task::{Context, Poll},
    Stream, StreamExt,
};
use owning_ref::OwningHandle;
use rdkafka::{
    config::{ClientConfig, RDKafkaLogLevel},
    consumer::{stream_consumer::StreamConsumer, Consumer, DefaultConsumerContext},
    error::KafkaResult,
    message::BorrowedMessage,
    util::Timeout,
    Message, TopicPartitionList,
};
use std::{
    fmt::{Debug, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    time::Duration,
};
use uuid::Uuid;

enum DeclaredContentType {
    Json,
    String,
    Other,
}

impl DeclaredContentType {
    fn detect(content_type: Option<&str>) -> Self {
        match content_type {
            Some(ct) if Self::is_json_content_type(ct) => Self::Json,
            Some(ct) if Self::is_text_content_type(ct) => Self::String,
            _ => Self::Other,
        }
    }

    /// Check if the content type indicates a JSON payload
    fn is_json_content_type(content_type: &str) -> bool {
        content_type.starts_with("application/json")
            || content_type.starts_with("text/json")
            || content_type.ends_with("+json")
    }

    /// Check if the content type indicates a plain text payload
    fn is_text_content_type(content_type: &str) -> bool {
        content_type.starts_with("text/plain")
    }
}

pub trait AckMode {
    fn configure(config: &mut ClientConfig);
}

pub struct AutoAck;
pub struct CustomAck;

impl AckMode for AutoAck {
    fn configure(config: &mut ClientConfig) {
        // automatically update offsets
        config.set("enable.auto.offset.store", "true");
    }
}

impl AckMode for CustomAck {
    fn configure(config: &mut ClientConfig) {
        // manually update offsets
        config.set("enable.auto.offset.store", "false");
    }
}

#[derive(Clone, Debug)]
pub struct EventStreamConfig {
    pub kafka: KafkaConfig,
    pub consumer_group: Option<String>,
}

pub struct EventStream<'s, Ack = AutoAck>
where
    Ack: AckMode,
{
    _marker: PhantomData<Ack>,
    upstream: OwningHandle<Box<StreamConsumer>, Box<rdkafka::consumer::MessageStream<'s>>>,
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
            .set("bootstrap.servers", &cfg.kafka.client.bootstrap_servers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            // automatically commit, the offsets
            .set("auto.commit.interval.ms", "5000")
            .set("enable.auto.commit", "true")
            // set logging
            .set_log_level(RDKafkaLogLevel::Info);

        // add custom properties

        for (k, v) in &cfg.kafka.client.properties {
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

        let topic = cfg.kafka.topic.clone();

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

        Ok(Self::wrap(topic, consumer))
    }

    fn new_with_group(cfg: &EventStreamConfig, group_id: String) -> Result<Self, EventStreamError> {
        let mut consumer = Self::new_config(cfg);
        consumer.set("group.id", &group_id);

        Ack::configure(&mut consumer);

        let consumer: StreamConsumer<DefaultConsumerContext> = consumer.create()?;

        log::debug!("Created consumer");

        consumer.subscribe(&[&cfg.kafka.topic])?;

        log::debug!("Subscribed");

        Ok(Self::wrap(cfg.kafka.topic.clone(), consumer))
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
        self.upstream.as_owner().store_offset_from_message(msg)
    }
}

/// Try to ensure that the data section is JSON encoded when the content type
/// indicated a JSON payload.
///
/// This is necessary as e.g. reading from Kafka, the payload will always be binary.
fn fixup_data_type(mut event: Event) -> Event {
    // Pre-flight check if we need to convert
    let converter = match (
        DeclaredContentType::detect(event.datacontenttype()),
        event.data(),
    ) {
        (DeclaredContentType::String, Some(Data::String(_))) => {
            return event;
        }
        (DeclaredContentType::Json, Some(Data::Json(_))) => {
            return event;
        }
        (DeclaredContentType::Other, _) => {
            return event;
        }
        (_, None) => {
            return event;
        }
        (DeclaredContentType::Json, Some(_)) => make_json,
        (DeclaredContentType::String, Some(_)) => make_string,
    };

    // we know now that the content is indicated as something different -> do the conversion

    let (content_type, schema_type, data) = match event.take_data() {
        (Some(content_type), schema_type, Some(data)) => {
            (Some(content_type), schema_type, Some(converter(data)))
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

/// Get JSON from the data section, ignore error, don't do checks if we don't need to.
fn make_json(data: Data) -> Data {
    match data {
        Data::String(ref str) => serde_json::from_str(str).map_or_else(|_| data, Data::Json),
        Data::Binary(ref slice) => serde_json::from_slice(slice).map_or_else(|_| data, Data::Json),
        json => json,
    }
}

/// Get string data from the data section, ignore error, don't do checks if we really need to.
fn make_string(data: Data) -> Data {
    match data {
        Data::Json(json) => Data::String(json.to_string()),
        Data::Binary(slice) => {
            String::from_utf8(slice).map_or_else(|err| Data::Binary(err.into_bytes()), Data::String)
        }
        string => string,
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
    event: T,
    msg: BorrowedMessage<'s>,
}

impl<'s, T> Handle<'s, T> {
    pub fn replace<U>(self, event: U) -> Handle<'s, U> {
        Handle {
            event,
            msg: self.msg,
        }
    }

    pub fn map<U, F>(self, f: F) -> Handle<'s, U>
    where
        F: Fn(T) -> U,
    {
        Handle {
            event: f(self.event),
            msg: self.msg,
        }
    }

    pub fn try_map<U, F, E>(self, f: F) -> Result<Handle<'s, U>, E>
    where
        F: Fn(T) -> Result<U, E>,
    {
        Ok(Handle {
            event: f(self.event)?,
            msg: self.msg,
        })
    }
}

impl<T> Deref for Handle<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.event
    }
}

impl<T> DerefMut for Handle<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.event
    }
}

impl Stream for EventStream<'_, AutoAck> {
    type Item = Result<Event, EventStreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next = self.upstream.poll_next_unpin(cx);

        match next {
            Poll::Pending => Poll::Pending,
            Poll::Ready(next) => match next {
                None => Poll::Ready(None),
                Some(Err(e)) => Poll::Ready(Some(Err(e.into()))),
                Some(Ok(msg)) => {
                    log::debug!(
                        "Message - partition: {}, offset: {}",
                        msg.partition(),
                        msg.offset()
                    );

                    let event = msg.to_event()?;
                    let event = fixup_data_type(event);

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
                    log::debug!(
                        "Message - partition: {}, offset: {}",
                        msg.partition(),
                        msg.offset()
                    );
                    let event = msg.to_event()?;
                    let event = fixup_data_type(event);

                    let event = Handle { event, msg };
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
            // is binary, convert to text, even though it is JSON
            (
                Some("text/plain"),
                Some(Data::Binary(r#"{"foo": "bar"}"#.as_bytes().into())),
                Some(Data::String(r#"{"foo": "bar"}"#.into())),
            ),
            // is binary, but unknown type, leave it
            (
                Some("something/different"),
                Some(Data::Binary(r#"{"foo": "bar"}"#.as_bytes().into())),
                Some(Data::Binary(r#"{"foo": "bar"}"#.as_bytes().into())),
            ),
        ] {
            let event = event(content_type, input);
            let (ct, st, data) = fixup_data_type(event).take_data();

            assert_eq!(ct.as_deref(), content_type);
            assert_eq!(st.as_ref().map(Url::as_str), Some("https://foo.bar/"));
            assert_eq!(data, output);
        }
    }
}
