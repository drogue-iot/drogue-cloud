use actix_web::error::PayloadError;
use actix_web::web;
use actix_web::web::Bytes;
use anyhow::Context;
use cloudevents::event::Data;
use cloudevents::{EventBuilder, EventBuilderV10};
use futures::StreamExt;
use futures_core::Stream;
use log;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Publish {
    pub channel: String,
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
pub struct HttpEndpoint {
    client: reqwest::Client,
    sink: String,
}

impl HttpEndpoint {
    pub fn new() -> anyhow::Result<Self> {
        let sink = std::env::var("K_SINK").context("Missing variable 'K_SINK'")?;

        Ok(HttpEndpoint {
            client: reqwest::ClientBuilder::new().build()?,
            sink,
        })
    }

    pub async fn publish<S>(
        &self,
        publish: Publish,
        mut body: S,
    ) -> Result<PublishResponse, actix_web::Error>
    where
        S: Stream<Item = Result<Bytes, PayloadError>> + Unpin,
    {
        let mut bytes = web::BytesMut::new();
        while let Some(item) = body.next().await {
            bytes.extend_from_slice(&item?);
        }
        let bytes = bytes.freeze();

        let event = EventBuilderV10::new()
            .id(uuid::Uuid::new_v4().to_string())
            .source("https://dentrassi.de/iot")
            .subject(&publish.channel)
            .ty("de.dentrassi.iot.message");

        // try decoding as JSON

        let event = match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) => event.data("text/json", Data::Json(v)),
            Err(_) => event.data("application/octet-stream", bytes.to_vec()),
        };

        // build event

        let event = event
            .build()
            .map_err(actix_web::error::ErrorInternalServerError)?;

        let response =
            cloudevents_sdk_reqwest::event_to_request(event, self.client.post(&self.sink))
                // Unable to build event ... fail internally
                .map_err(actix_web::error::ErrorInternalServerError)?
                .send()
                .await
                // Unable to process HTTP request ... fail internally
                .map_err(actix_web::error::ErrorInternalServerError)?;

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
}
