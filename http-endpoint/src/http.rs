use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};
use log;
use serde::{Deserialize, Serialize};

use actix_web::error::PayloadError;
use actix_web::web::Bytes;
use anyhow::Context;
use cloudevents::{EventBuilder, EventBuilderV10};
use futures::StreamExt;
use futures_core::Stream;

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
            .data("application/octet-stream", bytes.to_vec())
            .ty("de.dentrassi.iot.message");

        let event = event
            .build()
            .map_err(actix_web::error::ErrorInternalServerError)?;

        let response =
            cloudevents_sdk_reqwest::event_to_request(event, self.client.post(&self.sink))
                // If i can't build the request, fail with internal server error
                .map_err(actix_web::error::ErrorInternalServerError)?
                .send()
                .await
                // If something went wrong when sending the event, fail with internal server error
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
