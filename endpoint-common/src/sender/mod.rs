mod process;

pub use process::ExternalClientPoolConfig;

use crate::{
    sender::process::{ExternalClientPool, Outcome},
    sink::{Sink, SinkError, SinkTarget},
    EXT_PARTITIONKEY,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cloudevents::{event::Data, Event, EventBuilder, EventBuilderV10};
use drogue_client::{
    meta::v1::{NonScopedMetadata, ScopedMetadata},
    registry,
};
use drogue_cloud_service_api::{
    webapp::HttpResponse, EXT_APPLICATION_UID, EXT_DEVICE_UID, EXT_INSTANCE, EXT_SENDER,
    EXT_SENDER_UID,
};
use drogue_cloud_service_common::{metrics, Id, IdInjector};
use lazy_static::lazy_static;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use process::Processor;
use prometheus::{CounterVec, Opts};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use tracing::instrument;

lazy_static! {
    pub static ref DOWNSTREAM_EVENTS_COUNTER: CounterVec = CounterVec::new(
        Opts::new("drogue_downstream_events", "Downstream events"),
        &["endpoint", "outcome"]
    )
    .unwrap();
}

const DEFAULT_TYPE_EVENT: &str = "io.drogue.event.v1";

#[derive(Clone, Debug)]
pub struct Publish<'a> {
    pub application: &'a registry::v1::Application,
    /// The device id this message originated from.
    pub device: PublishId,
    /// The device id this message was sent by.
    ///
    /// In case of a gateway sending for another device, this would be the gateway id. In case
    /// of a device sending for its own, this would be equal to the device_id.
    pub sender: PublishId,
    pub channel: String,
    pub options: PublishOptions,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PublishId {
    pub name: String,
    pub uid: Option<String>,
}

pub struct PublishIdPair {
    /// The device which originally sent the event
    pub device: PublishId,
    /// The device which transmitted the event
    pub sender: PublishId,
}

impl PublishIdPair {
    /// Create a new pair of publish IDs for a typical (connected) device and as-device combination.
    ///
    /// If the `as` device is provided, then the connected device is used as sender and the `as`
    /// device is used as device.
    ///
    /// If no `as` device is provided, both sender and device will be set to the device id.
    pub fn with_devices(
        device: registry::v1::Device,
        r#as: Option<registry::v1::Device>,
    ) -> PublishIdPair {
        let device_id = match r#as {
            // use the "as" information as device id
            Some(r#as) => r#as.metadata.to_id(),
            // use the original device id
            None => device.metadata.to_id(),
        };
        PublishIdPair {
            device: device_id,
            sender: device.metadata.into_id(),
        }
    }
}

pub trait IntoPublishId {
    fn into_id(self) -> PublishId;
}

pub trait ToPublishId {
    fn to_id(&self) -> PublishId;
}

macro_rules! into_impl_into {
    ($name:ty) => {
        impl IntoPublishId for $name {
            fn into_id(self) -> PublishId {
                PublishId {
                    name: self.into(),
                    uid: None,
                }
            }
        }
    };
}
macro_rules! to_impl_into {
    ($name:ty) => {
        impl ToPublishId for $name {
            fn to_id(&self) -> PublishId {
                PublishId {
                    name: self.into(),
                    uid: None,
                }
            }
        }
    };
}

into_impl_into!(String);
to_impl_into!(str);
to_impl_into!(String);

impl ToPublishId for ScopedMetadata {
    fn to_id(&self) -> PublishId {
        PublishId {
            name: self.name.clone(),
            uid: Some(self.uid.clone()),
        }
    }
}

impl ToPublishId for NonScopedMetadata {
    fn to_id(&self) -> PublishId {
        PublishId {
            name: self.name.clone(),
            uid: Some(self.uid.clone()),
        }
    }
}

impl IntoPublishId for ScopedMetadata {
    fn into_id(self) -> PublishId {
        PublishId {
            name: self.name,
            uid: Some(self.uid),
        }
    }
}

impl IntoPublishId for NonScopedMetadata {
    fn into_id(self) -> PublishId {
        PublishId {
            name: self.name,
            uid: Some(self.uid),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublishOptions {
    pub time: Option<DateTime<Utc>>,
    pub topic: Option<String>,
    pub data_schema: Option<String>,
    pub content_type: Option<String>,
    pub extensions: HashMap<String, String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum PublishOutcome {
    /// Message accepted
    Accepted,
    /// Invalid message format
    Rejected,
    /// Input queue full
    QueueFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Cloud to device messaging
    Upstream,
    /// Device to cloud messaging
    Downstream,
}

/// A sender delivering events upstream, from the cloud to the device.
#[derive(Debug, Clone)]
pub struct UpstreamSender<S>
where
    S: Sink,
{
    sink: S,
    instance: String,
    pool: ExternalClientPool,
}

impl<S> UpstreamSender<S>
where
    S: Sink,
{
    pub fn new<I: Into<String>>(
        instance: I,
        sink: S,
        config: ExternalClientPoolConfig,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            sink,
            instance: instance.into(),
            pool: ExternalClientPool::new(config),
        })
    }
}

/// A sender delivering events downstream, from the device to the cloud.
#[derive(Debug, Clone)]
pub struct DownstreamSender<S>
where
    S: Sink,
{
    sink: S,
    instance: String,
    pool: ExternalClientPool,
}

impl<S> DownstreamSender<S>
where
    S: Sink,
{
    pub fn new(
        sink: S,
        instance: String,
        config: ExternalClientPoolConfig,
    ) -> anyhow::Result<Self> {
        metrics::register(Box::new(DOWNSTREAM_EVENTS_COUNTER.clone()))?;
        Ok(Self {
            sink,
            instance,
            pool: ExternalClientPool::new(config),
        })
    }
}

#[derive(Error, Debug)]
pub enum PublishError<E: std::error::Error + 'static> {
    #[error("Sink error")]
    Sink(#[from] SinkError<E>),
    #[error("Publish spec error")]
    Spec(#[source] serde_json::Error),
    #[error("Build event error")]
    Event(#[source] cloudevents::event::EventBuilderError),
    #[error("Process error")]
    Processor(#[from] process::Error),
}

#[async_trait]
impl<S> Publisher<S> for DownstreamSender<S>
where
    S: Sink,
{
    fn instance(&self) -> String {
        self.instance.clone()
    }

    fn pool(&self) -> ExternalClientPool {
        self.pool.clone()
    }

    #[inline]
    fn direction() -> Direction {
        Direction::Downstream
    }

    async fn send(
        &self,
        app: &registry::v1::Application,
        event: Event,
    ) -> Result<PublishOutcome, SinkError<S::Error>> {
        self.sink.publish(SinkTarget::Events(app), event).await
    }
}

#[async_trait]
impl<S> Publisher<S> for UpstreamSender<S>
where
    S: Sink,
{
    fn instance(&self) -> String {
        self.instance.clone()
    }

    fn pool(&self) -> ExternalClientPool {
        self.pool.clone()
    }

    #[inline]
    fn direction() -> Direction {
        Direction::Upstream
    }

    async fn send(
        &self,
        app: &registry::v1::Application,
        event: Event,
    ) -> Result<PublishOutcome, SinkError<S::Error>> {
        self.sink.publish(SinkTarget::Commands(app), event).await
    }
}

#[async_trait]
pub trait Publisher<S>
where
    S: Sink,
{
    fn instance(&self) -> String;

    fn pool(&self) -> ExternalClientPool;

    fn direction() -> Direction;

    async fn send(
        &self,
        app: &registry::v1::Application,
        event: Event,
    ) -> Result<PublishOutcome, SinkError<S::Error>>;

    #[allow(clippy::needless_lifetimes)]
    #[instrument(
        level = "debug",
        skip(self,publish,body),
        field(
            application=publish.application.metadata.name,
            sender=publish.sender,
            device=publish.device,
            channel=publish.channel,
            body_length=body.len()
        ),
        ret,
        err
    )]
    async fn publish<'a, B>(
        &self,
        publish: Publish<'a>,
        body: B,
    ) -> Result<PublishOutcome, PublishError<S::Error>>
    where
        B: AsRef<[u8]> + Send + Sync,
    {
        let app_id = publish.application.metadata.name.clone();
        let app_enc = utf8_percent_encode(&app_id, NON_ALPHANUMERIC);
        let device_enc = utf8_percent_encode(&publish.device.name, NON_ALPHANUMERIC);

        let source = format!("{}/{}", app_enc, device_enc);

        let mut event = EventBuilderV10::new()
            .id(uuid::Uuid::new_v4().to_string())
            .ty(DEFAULT_TYPE_EVENT)
            // we need an "absolute" URL for the moment: until 0.4 is released
            // see: https://github.com/cloudevents/sdk-rust/issues/106
            .source(format!("drogue://{}", source))
            .inject(Id::new(app_id, publish.device.name))
            .subject(&publish.channel)
            .time(Utc::now());

        event = event.extension(
            EXT_APPLICATION_UID,
            publish.application.metadata.uid.clone(),
        );

        if let Some(uid) = publish.device.uid {
            event = event.extension(EXT_DEVICE_UID, uid);
        }
        if let Some(uid) = publish.sender.uid {
            event = event.extension(EXT_SENDER_UID, uid);
        }

        event = event.extension(EXT_PARTITIONKEY, source);
        event = event.extension(EXT_INSTANCE, self.instance());
        event = event.extension(EXT_SENDER, publish.sender.name);

        if let Some(data_schema) = publish.options.data_schema {
            event = event.extension("dataschema", data_schema);
        }

        for (k, v) in publish.options.extensions {
            event = event.extension(&k, v);
        }

        log::debug!("Content-Type: {:?}", publish.options.content_type);
        log::debug!("Payload size: {} bytes", body.as_ref().len());

        let event = match publish.options.content_type {
            // if the content type "is JSON", we do an extra check if the content type is indeed JSON
            Some(t) if is_json(&t) => {
                // try decoding as JSON
                match serde_json::from_slice::<Value>(body.as_ref()) {
                    // ok -> pass along
                    Ok(v) => event.data(mime::APPLICATION_JSON.to_string(), Data::Json(v)),
                    // not ok -> reject
                    Err(_) => return Ok(PublishOutcome::Rejected),
                }
            }
            // pass through content type
            Some(t) => event.data(t, Vec::from(body.as_ref())),
            // no content type, try JSON, then fall back to "bytes"
            None => {
                // try decoding as JSON
                match serde_json::from_slice::<Value>(body.as_ref()) {
                    Ok(v) => event.data(mime::APPLICATION_JSON.to_string(), Data::Json(v)),
                    Err(_) => event.data(
                        mime::APPLICATION_OCTET_STREAM.to_string(),
                        Vec::from(body.as_ref()),
                    ),
                }
            }
        };

        let event = event.build().map_err(PublishError::Event)?;

        // handle publish steps

        let processor = Processor::try_from((Self::direction(), publish.application, self.pool()))
            .map_err(PublishError::Spec)?;
        match processor.process(event).await? {
            Outcome::Rejected(reason) => {
                // event was rejected
                log::debug!("Event rejected: {}", reason);
                Ok(PublishOutcome::Rejected)
            }
            Outcome::Accepted(event) => {
                // event was accepted, send it
                Ok(self.send(publish.application, event).await?)
            }
            Outcome::Dropped => {
                // event was dropped, skip it
                log::debug!("Outcome is to drop event");
                Ok(PublishOutcome::Accepted)
            }
        }
    }

    #[allow(clippy::needless_lifetimes)]
    #[allow(clippy::async_yields_async)]
    async fn publish_http_default<'a, B>(&self, publish: Publish<'a>, body: B) -> HttpResponse
    where
        B: AsRef<[u8]> + Send + Sync,
    {
        match self.publish(publish, body).await {
            Ok(PublishOutcome::Accepted) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "Accepted"])
                    .inc();
                HttpResponse::Accepted().finish()
            }
            Ok(PublishOutcome::Rejected) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "Rejected"])
                    .inc();
                HttpResponse::NotAcceptable().finish()
            }
            Ok(PublishOutcome::QueueFull) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "QueueFull"])
                    .inc();
                HttpResponse::ServiceUnavailable().finish()
            }
            Err(err) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "Error"])
                    .inc();
                HttpResponse::InternalServerError()
                    .content_type("text/plain")
                    .body(err.to_string())
            }
        }
    }
}

pub(crate) fn is_json(content_type: &str) -> bool {
    content_type.starts_with("application/json")
        || content_type.starts_with("text/json")
        || content_type.ends_with("+json")
}
