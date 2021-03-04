use crate::{Event, EventSender, SenderResult};
use async_trait::async_trait;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub enum MockSenderError {
    Lock,
}

impl Display for MockSenderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("MockSenderError")
    }
}

impl std::error::Error for MockSenderError {}

/// A mock sender for testing.
#[derive(Debug, Clone)]
pub struct MockEventSender {
    events: Arc<RwLock<Vec<Event>>>,
}

impl Default for MockEventSender {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEventSender {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get the current events.
    pub fn events(&self) -> SenderResult<Vec<Event>, MockSenderError> {
        Ok(self
            .events
            .read()
            .map_err(|_| MockSenderError::Lock)?
            .clone())
    }

    // Clear currently recorded events.
    pub fn reset(&mut self) -> SenderResult<(), MockSenderError> {
        self.events
            .write()
            .map_err(|_| MockSenderError::Lock)?
            .clear();
        Ok(())
    }

    /// Get and reset the current events.
    pub fn retrieve(&mut self) -> SenderResult<Vec<Event>, MockSenderError> {
        let events = self.events()?;
        self.reset()?;
        Ok(events)
    }
}

/// Implement by storing events in an internal list.
#[async_trait]
impl EventSender for MockEventSender {
    type Error = MockSenderError;

    async fn notify<I>(&self, events: I) -> SenderResult<(), Self::Error>
    where
        I: IntoIterator<Item = Event> + Sync + Send,
    {
        self.events
            .write()
            .map_err(|_| MockSenderError::Lock)?
            .extend(events);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::mock::MockEventSender;
    use crate::{Event, EventSender, SendEvent};

    #[tokio::test]
    async fn test_1() {
        let sender = MockEventSender::new();
        let mut sender_cloned = sender.clone();

        assert!(sender.notify(vec![]).await.is_ok());

        let events = sender.events();
        assert!(events.is_ok());
        assert_eq!(events.unwrap(), vec![]);

        assert!(Event::Application {
            instance: "instance1".into(),
            application: "app1".into(),
            generation: 123,
            uid: "a".into(),
            path: "spec/core".into()
        }
        .send_with(&sender)
        .await
        .is_ok());

        let events = sender.events();
        assert!(events.is_ok());
        assert_eq!(
            events.unwrap(),
            vec![Event::Application {
                instance: "instance1".into(),
                application: "app1".into(),
                generation: 123,
                uid: "a".into(),
                path: "spec/core".into()
            }]
        );

        // expecting the same events from the cloned instance.

        let events_cloned = sender_cloned.retrieve();

        assert!(events_cloned.is_ok());
        assert_eq!(
            events_cloned.unwrap(),
            vec![Event::Application {
                instance: "instance1".into(),
                application: "app1".into(),
                generation: 123,
                uid: "a".into(),
                path: "spec/core".into()
            }]
        );

        // both should be empty now

        let events = sender.events();
        assert!(events.is_ok());
        assert_eq!(events.unwrap(), vec![]);

        let events_cloned = sender.events();
        assert!(events_cloned.is_ok());
        assert_eq!(events_cloned.unwrap(), vec![]);
    }
}
