mod process;

use crate::{
    sender::process::Outcome,
    sink::{Sink, SinkError, SinkTarget},
    EXT_PARTITIONKEY,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cloudevents::{event::Data, Event, EventBuilder, EventBuilderV10};
use drogue_client::registry;
use drogue_cloud_service_api::webapp::HttpResponse;
use drogue_cloud_service_api::{EXT_INSTANCE, EXT_SENDER};
use drogue_cloud_service_common::{Id, IdInjector};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use process::Processor;
use prometheus::{CounterVec, Opts};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

use lazy_static::lazy_static;
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
    pub device_id: String,
    /// The device id this message was sent by.
    ///
    /// In case of a gateway sending for another device, this would be the gateway id. In case
    /// of a device sending for its own, this would be equal to the device_id.
    pub sender_id: String,
    pub channel: String,
    pub options: PublishOptions,
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

/// A sender delivering events upstream, from the cloud to the device.
#[derive(Debug, Clone)]
pub struct UpstreamSender<S>
where
    S: Sink,
{
    sink: S,
    instance: String,
}

impl<S> UpstreamSender<S>
where
    S: Sink,
{
    pub fn new<I: Into<String>>(instance: I, sink: S) -> anyhow::Result<Self> {
        Ok(Self {
            sink,
            instance: instance.into(),
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
}

impl<S> DownstreamSender<S>
where
    S: Sink,
{
    pub fn new(sink: S, instance: String) -> anyhow::Result<Self> {
        prometheus::default_registry()
            .register(Box::new(DOWNSTREAM_EVENTS_COUNTER.clone()))
            .unwrap();
        Ok(Self { sink, instance })
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

    async fn send(
        &self,
        app: &registry::v1::Application,
        event: Event,
    ) -> Result<PublishOutcome, SinkError<S::Error>>;

    #[allow(clippy::needless_lifetimes)]
    #[instrument(skip(self,body),field(body_length=body.len()))]
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
        let device_enc = utf8_percent_encode(&publish.device_id, NON_ALPHANUMERIC);

        let source = format!("{}/{}", app_enc, device_enc);

        let mut event = EventBuilderV10::new()
            .id(uuid::Uuid::new_v4().to_string())
            .ty(DEFAULT_TYPE_EVENT)
            // we need an "absolute" URL for the moment: until 0.4 is released
            // see: https://github.com/cloudevents/sdk-rust/issues/106
            .source(format!("drogue://{}", source))
            .inject(Id::new(app_id, publish.device_id))
            .subject(&publish.channel)
            .time(Utc::now());

        event = event.extension(EXT_PARTITIONKEY, source);
        event = event.extension(EXT_INSTANCE, self.instance());
        event = event.extension(EXT_SENDER, publish.sender_id);

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

        let processor = Processor::try_from(publish.application).map_err(PublishError::Spec)?;
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
