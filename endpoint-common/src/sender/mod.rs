use crate::error::HttpEndpointError;
use crate::sink::{Sink, SinkError, SinkTarget};
use actix_web::HttpResponse;
use anyhow::Context;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cloudevents::{event::Data, Event, EventBuilder, EventBuilderV10};
use drogue_client::registry;
use drogue_cloud_service_api::EXT_INSTANCE;
use drogue_cloud_service_common::{Id, IdInjector};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, future::Future};

const DEFAULT_TYPE_EVENT: &str = "io.drogue.event.v1";

#[derive(Clone, Debug)]
pub struct Publish<'a> {
    pub application: &'a registry::v1::Application,
    pub device_id: String,
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
    pub fn new(sink: S) -> anyhow::Result<Self> {
        let instance = std::env::var("INSTANCE").context("Missing variable 'INSTANCE'")?;

        Ok(Self { sink, instance })
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
        Ok(Self { sink, instance })
    }
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
    async fn publish<'a, B>(
        &self,
        publish: Publish<'a>,
        body: B,
    ) -> Result<PublishOutcome, SinkError<S::Error>>
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

        event = event.extension(crate::EXT_PARTITIONKEY, source);
        event = event.extension(EXT_INSTANCE, self.instance());

        if let Some(data_schema) = publish.options.data_schema {
            event = event.extension("dataschema", data_schema);
        }

        for (k, v) in publish.options.extensions {
            event = event.extension(&k, v);
        }

        log::debug!("Content-Type: {:?}", publish.options.content_type);
        log::debug!("Payload size: {} bytes", body.as_ref().len());

        let event = match publish.options.content_type {
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

        // build event

        self.send(&publish.application, event.build()?).await
    }

    #[allow(clippy::needless_lifetimes)]
    async fn publish_http<'a, B, H, F>(
        &self,
        publish: Publish<'a>,
        body: B,
        f: H,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send + Sync,
        H: FnOnce(PublishOutcome) -> F + Send + Sync,
        F: Future<Output = Result<HttpResponse, HttpEndpointError>> + Send + Sync,
    {
        match self.publish(publish, body).await {
            // ok
            Ok(outcome) => f(outcome).await,

            // internal error
            Err(err) => Ok(HttpResponse::InternalServerError()
                .content_type("text/plain")
                .body(err.to_string())),
        }
    }

    #[allow(clippy::needless_lifetimes)]
    async fn publish_http_default<'a, B>(
        &self,
        publish: Publish<'a>,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send + Sync,
    {
        self.publish_http(publish, body, |outcome| async move {
            match outcome {
                PublishOutcome::Accepted => Ok(HttpResponse::Accepted().finish()),
                PublishOutcome::Rejected => Ok(HttpResponse::NotAcceptable().finish()),
                PublishOutcome::QueueFull => Ok(HttpResponse::ServiceUnavailable().finish()),
            }
        })
        .await
    }
}
