use crate::error::HttpEndpointError;
use actix_web::HttpResponse;
use anyhow::Context;
use chrono::{DateTime, Utc};
use cloudevents::{event::Data, EventBuilder, EventBuilderV10};
use drogue_cloud_service_common::{Id, IdInjector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Publish {
    pub channel: String,
    pub app_id: String,
    pub device_id: String,
    pub options: PublishOptions,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PublishOptions {
    pub time: Option<DateTime<Utc>>,
    pub topic: Option<String>,
    pub model_id: Option<String>,
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
    client: reqwest::Client,
    sink: String,
}

impl DownstreamSender {
    pub fn new() -> anyhow::Result<Self> {
        let sink = std::env::var("K_SINK").context("Missing variable 'K_SINK'")?;

        Ok(DownstreamSender {
            client: reqwest::ClientBuilder::new().build()?,
            sink,
        })
    }

    pub async fn publish<B>(&self, publish: Publish, body: B) -> anyhow::Result<PublishResponse>
    where
        B: AsRef<[u8]>,
    {
        let partitionkey = format!("{}/{}", publish.app_id, publish.device_id);

        let mut event = EventBuilderV10::new()
            .id(uuid::Uuid::new_v4().to_string())
            .source("https://drogue.io/endpoint")
            .inject(Id::new(publish.app_id, publish.device_id))
            .subject(&publish.channel)
            .time(Utc::now())
            .ty("io.drogue.iot.message");

        event = event.extension("partitionkey", partitionkey);

        if let Some(model_id) = publish.options.model_id {
            event = event.extension("modelid", model_id);
        }

        for (k, v) in publish.options.extensions {
            event = event.extension(&k, v);
        }

        log::debug!("Content-Type: {:?}", publish.options.content_type);
        log::debug!("Payload size: {} bytes", body.as_ref().len());

        let event = match publish.options.content_type {
            Some(t) => event.data(t, Vec::from(body.as_ref())),
            None => {
                // try decoding as JSON
                match serde_json::from_slice::<Value>(body.as_ref()) {
                    Ok(v) => event.data("application/json", Data::Json(v)),
                    Err(_) => event.data("application/octet-stream", Vec::from(body.as_ref())),
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
