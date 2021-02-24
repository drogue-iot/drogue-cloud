use crate::{Event, EventSender, EventSenderError, SenderResult};
use async_trait::async_trait;
use cloudevents_sdk_reqwest::event_to_request;
use reqwest::{Client, Url};
use std::convert::TryInto;

#[derive(Clone, Debug)]
pub struct ReqwestEventSender {
    client: Client,
    url: Url,
}

impl ReqwestEventSender {
    /// Create a new reqwest based instance.
    pub fn new(client: Client, url: Url) -> Self {
        Self { client, url }
    }
}

#[async_trait]
impl EventSender for ReqwestEventSender {
    type Error = reqwest::Error;

    async fn notify<I>(&self, events: I) -> SenderResult<(), Self::Error>
    where
        I: IntoIterator<Item = Event> + Sync + Send,
    {
        let events = events.into_iter().collect::<Vec<_>>();
        for event in events {
            let event: cloudevents::Event = event.try_into().map_err(EventSenderError::Event)?;
            event_to_request(event, self.client.post(self.url.clone()))
                .map_err(EventSenderError::CloudEvent)?
                .send()
                .await?;
        }

        Ok(())
    }
}
