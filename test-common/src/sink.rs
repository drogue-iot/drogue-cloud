use async_trait::async_trait;
use drogue_cloud_endpoint_common::{
    sender::PublishOutcome,
    sink::{Sink, SinkError, SinkTarget},
};
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct MockSink {
    pub commands: Arc<RwLock<Vec<cloudevents::event::Event>>>,
    pub events: Arc<RwLock<Vec<cloudevents::event::Event>>>,
}

impl MockSink {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn commands(&self) -> Vec<cloudevents::event::Event> {
        self.commands.read().unwrap().clone()
    }

    pub async fn events(&self) -> Vec<cloudevents::event::Event> {
        self.events.read().unwrap().clone()
    }
}

impl Default for MockSink {
    fn default() -> Self {
        Self {
            commands: Arc::new(RwLock::new(Vec::new())),
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl Sink for MockSink {
    #[allow(clippy::needless_lifetimes)]
    async fn publish<'a>(
        &self,
        target: SinkTarget<'a>,
        event: cloudevents::event::Event,
    ) -> Result<PublishOutcome, SinkError> {
        match target {
            SinkTarget::Events(_) => {
                self.events.write().unwrap().push(event);
            }
            SinkTarget::Commands(_) => {
                self.commands.write().unwrap().push(event);
            }
        }

        Ok(PublishOutcome::Accepted)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cloudevents::EventBuilder;

    #[tokio::test]
    async fn test_mock() {
        let sink = MockSink::new();
        let sink2 = sink.clone();

        let app = Default::default();
        let event = cloudevents::event::EventBuilderV10::new()
            .id("1")
            .ty("type")
            .source("foo:/bar")
            .build()
            .unwrap();

        sink2
            .publish(SinkTarget::Events(&app), event)
            .await
            .unwrap();

        let events = sink.events().await;

        assert_eq!(1, events.len());
    }
}
