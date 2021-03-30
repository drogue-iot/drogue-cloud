use crate::error::HttpEndpointError;
use actix_web::HttpResponse;
use anyhow::Context;
use chrono::{DateTime, Utc};
use cloudevents::{event::Data, EventBuilder, EventBuilderV10};
use drogue_cloud_service_api::EXT_INSTANCE;
use drogue_cloud_service_common::{Id, IdInjector};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;

const DEFAULT_TYPE_EVENT: &str = "io.drogue.event.v1";

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
pub enum Outcome {
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishResponse {
    pub outcome: Outcome,
}

#[derive(Clone, Debug)]
pub struct DownstreamSender {
    pub client: reqwest::Client,
    sink: String,
    instance: String,
}

impl DownstreamSender {
    pub fn new() -> anyhow::Result<Self> {
        let sink = std::env::var("K_SINK").context("Missing variable 'K_SINK'")?;
        let instance = std::env::var("INSTANCE").context("Missing variable 'INSTANCE'")?;

        Ok(DownstreamSender {
            client: reqwest::ClientBuilder::new().build()?,
            sink,
            instance,
        })
    }

    pub async fn publish<B>(&self, publish: Publish, body: B) -> anyhow::Result<PublishResponse>
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
            .inject(Id::new(publish.app_id, publish.device_id))
            .subject(&publish.channel)
            .time(Utc::now());

        event = event.extension("partitionkey", source);
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

        let event = event.build()?;

        let response =
            cloudevents_sdk_reqwest::event_to_request(event, self.client.post(&self.sink))
                .map_err(|err| anyhow::anyhow!("{}", err.to_string()))
                .context("Failed to build event")?
                .send()
                .await
                .context("Failed to perform HTTP request")?;

        log::info!("Publish result: {:?}", response);

        match response.status().is_success() {
            true => Ok(PublishResponse {
                outcome: Outcome::Accepted,
            }),
            false => Ok(PublishResponse {
                outcome: Outcome::Rejected,
            }),
        }
    }

    pub async fn publish_http<B, H, F>(
        &self,
        publish: Publish,
        body: B,
        f: H,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]>,
        // F: FnOnce(Outcome) -> Result<HttpResponse, HttpEndpointError>,
        H: FnOnce(Outcome) -> F,
        F: Future<Output = Result<HttpResponse, HttpEndpointError>>,
    {
        match self.publish(publish, body).await {
            // ok
            Ok(PublishResponse { outcome }) => f(outcome).await,

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
                Outcome::Accepted => Ok(HttpResponse::Accepted().finish()),
                Outcome::Rejected => Ok(HttpResponse::NotAcceptable().finish()),
            }
        })
        .await
    }
}
