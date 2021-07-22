mod http;
mod kafka;

pub use self::http::HttpSink;
pub use kafka::*;

use crate::error::HttpEndpointError;
use actix_web::HttpResponse;
use anyhow::Context;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cloudevents::{event::Data, Event, EventBuilder, EventBuilderV10};
use drogue_cloud_service_api::events::EventTarget;
use drogue_cloud_service_api::EXT_INSTANCE;
use drogue_cloud_service_common::{Id, IdInjector};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use thiserror::Error;

const DEFAULT_TYPE_EVENT: &str = "io.drogue.event.v1";

const EXT_PARTITIONKEY: &str = "partitionkey";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Publish {
    pub app_id: String,
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

#[async_trait]
pub trait DownstreamSink: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + 'static;

    /// Publish an event.
    async fn publish(
        &self,
        target: EventTarget,
        event: Event,
    ) -> Result<PublishOutcome, DownstreamError<Self::Error>>;
}

#[derive(Error, Debug)]
pub enum DownstreamError<E: std::error::Error + 'static> {
    #[error("Build event error")]
    Build(#[from] cloudevents::event::EventBuilderError),
    #[error("Event error")]
    Event(#[from] cloudevents::message::Error),
    #[error("Transport error")]
    Transport(#[source] E),
}

#[derive(Debug, Clone, Copy)]
pub enum Target {
    Events,
    Commands,
}

impl Target {
    pub fn translate(&self, app: String) -> EventTarget {
        match self {
            Self::Commands => EventTarget::Commands(app),
            Self::Events => EventTarget::Events(app),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownstreamSender<S>
where
    S: DownstreamSink,
{
    sink: S,
    instance: String,
    target: Target,
}

impl<S> DownstreamSender<S>
where
    S: DownstreamSink,
{
    pub fn new(sink: S, target: Target) -> anyhow::Result<Self> {
        let instance = std::env::var("INSTANCE").context("Missing variable 'INSTANCE'")?;

        Ok(Self {
            sink,
            instance,
            target,
        })
    }

    pub async fn publish<B>(
        &self,
        publish: Publish,
        body: B,
    ) -> Result<PublishOutcome, DownstreamError<S::Error>>
    where
        B: AsRef<[u8]>,
    {
        let app_enc = utf8_percent_encode(&publish.app_id, NON_ALPHANUMERIC);
        let device_enc = utf8_percent_encode(&publish.device_id, NON_ALPHANUMERIC);

        let source = format!("{}/{}", app_enc, device_enc);

        let mut event = EventBuilderV10::new()
            .id(uuid::Uuid::new_v4().to_string())
            .ty(DEFAULT_TYPE_EVENT)
            // we need an "absolute" URL for the moment: until 0.4 is released
            // see: https://github.com/cloudevents/sdk-rust/issues/106
            .source(format!("drogue://{}", source))
            .inject(Id::new(publish.app_id.clone(), publish.device_id))
            .subject(&publish.channel)
            .time(Utc::now());

        event = event.extension(EXT_PARTITIONKEY, source);
        event = event.extension(EXT_INSTANCE, self.instance.clone());

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

        self.sink
            .publish(self.target.translate(publish.app_id), event.build()?)
            .await
    }

    pub async fn publish_http<B, H, F>(
        &self,
        publish: Publish,
        body: B,
        f: H,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]>,
        H: FnOnce(PublishOutcome) -> F,
        F: Future<Output = Result<HttpResponse, HttpEndpointError>>,
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

    pub async fn publish_http_default<B>(
        &self,
        publish: Publish,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]>,
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
