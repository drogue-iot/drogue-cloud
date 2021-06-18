use super::*;

use anyhow::Context;
use async_trait::async_trait;

#[derive(Clone, Debug)]
pub struct HttpSink {
    client: reqwest::Client,
    sink: String,
}

#[async_trait]
impl DownstreamSink for HttpSink {
    type Error = reqwest::Error;

    async fn publish(&self, event: Event) -> Result<PublishOutcome, DownstreamError<Self::Error>> {
        let response =
            cloudevents_sdk_reqwest::event_to_request(event, self.client.post(&self.sink))?
                .send()
                .await
                .map_err(|err| DownstreamError::Transport(err))?;

        log::debug!("Publish result: {:?}", response);

        match response.status().is_success() {
            true => Ok(PublishOutcome::Accepted),
            false => Ok(PublishOutcome::Rejected),
        }
    }
}

impl HttpSink {
    pub fn new() -> anyhow::Result<Self> {
        let sink = std::env::var("K_SINK").context("Missing variable 'K_SINK'")?;

        Ok(Self {
            client: reqwest::ClientBuilder::new().build()?,
            sink,
        })
    }
}
