use anyhow::Context;
use chrono::Utc;
use cloudevents::event::Data;
use cloudevents::{EventBuilder, EventBuilderV10};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use actix_web::HttpResponse;

use crate::error::HttpEndpointError;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Publish {
    pub channel: String,
    pub device_id: String,
    pub model_id: Option<String>,
    pub content_type: Option<String>,
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
        let mut event = EventBuilderV10::new()
            .id(uuid::Uuid::new_v4().to_string())
            .source("https://drogue.io/endpoint")
            .extension("device_id", publish.device_id)
            .subject(&publish.channel)
            .time(Utc::now())
            .ty("io.drogue.iot.message");

        if let Some(model_id) = publish.model_id {
            event = event.extension("model_id", model_id);
        }

        log::info!("Content-Type: {:?}", publish.content_type);

        let event = match publish.content_type {
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

    pub async fn publish_http<B>(
        &self,
        publish: Publish,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]>,
    {
        match self.publish(publish, body).await {
            // ok, and accepted
            Ok(PublishResponse {
                outcome: Outcome::Accepted,
            }) => Ok(HttpResponse::Accepted().finish()),

            // ok, but rejected
            Ok(PublishResponse {
                outcome: Outcome::Rejected,
            }) => Ok(HttpResponse::NotAcceptable().finish()),

            // internal error
            Err(err) => Ok(HttpResponse::InternalServerError()
                .content_type("text/plain")
                .body(err.to_string())),
        }
    }
}
