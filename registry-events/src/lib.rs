pub mod mock;
#[cfg(feature = "reqwest")]
pub mod reqwest;

use async_trait::async_trait;
use chrono::Utc;
use cloudevents::{AttributesReader, EventBuilder};
use drogue_cloud_service_api::{EXT_APPLICATION, EXT_DEVICE, EXT_INSTANCE};
use std::convert::TryInto;
use thiserror::Error;

const EXT_PARTITIONKEY: &str = "partitionkey";

const EVENT_TYPE_APPLICATION: &str = "io.drogue.registry.change.application";
const EVENT_TYPE_DEVICE: &str = "io.drogue.registry.change.device";

fn missing_field(field: &str) -> EventError {
    EventError::Parse(format!("Missing field: '{}'", field))
}

/// A registry event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Application {
        instance: String,
        id: String,
        path: String,
    },
    Device {
        instance: String,
        application: String,
        id: String,
        path: String,
    },
}

#[async_trait]
pub trait SendEvent<S: EventSender> {
    async fn send_with(self, sender: &S) -> SenderResult<(), S::Error>;
}

#[async_trait]
impl<S: EventSender> SendEvent<S> for Event {
    async fn send_with(self, sender: &S) -> SenderResult<(), S::Error> {
        sender.notify(vec![self; 1]).await
    }
}

#[async_trait]
impl<S: EventSender> SendEvent<S> for Vec<Event> {
    async fn send_with(self, sender: &S) -> SenderResult<(), S::Error> {
        sender.notify(self).await
    }
}

impl Event {
    fn from_app(event: cloudevents::Event) -> Result<Event, EventError> {
        Ok(Event::Application {
            instance: event
                .extension(EXT_INSTANCE)
                .ok_or_else(|| missing_field(EXT_INSTANCE))?
                .to_string(),
            id: event
                .extension(EXT_APPLICATION)
                .ok_or_else(|| missing_field(EXT_APPLICATION))?
                .to_string(),
            path: event
                .subject()
                .ok_or_else(|| missing_field("subject"))?
                .to_string(),
        })
    }

    fn from_device(event: cloudevents::Event) -> Result<Event, EventError> {
        Ok(Event::Device {
            instance: event
                .extension(EXT_INSTANCE)
                .ok_or_else(|| missing_field(EXT_INSTANCE))?
                .to_string(),
            application: event
                .extension(EXT_APPLICATION)
                .ok_or_else(|| missing_field(EXT_APPLICATION))?
                .to_string(),
            id: event
                .extension(EXT_DEVICE)
                .ok_or_else(|| missing_field(EXT_DEVICE))?
                .to_string(),
            path: event
                .subject()
                .ok_or_else(|| missing_field("subject"))?
                .to_string(),
        })
    }

    /// Help creating new events.
    fn new_change<C>(paths: Vec<String>, c: C) -> Vec<Event>
    where
        C: Fn(String) -> Event,
    {
        if paths.is_empty() {
            vec![c(".".to_string())]
        } else {
            paths.into_iter().map(|path| c(path)).collect()
        }
    }

    /// create new events for an app
    pub fn new_app<I, A>(instance_id: I, app_id: A, paths: Vec<String>) -> Vec<Event>
    where
        I: ToString,
        A: ToString,
    {
        Self::new_change(paths, |path| Event::Application {
            instance: instance_id.to_string(),
            id: app_id.to_string(),
            path,
        })
    }

    /// create new events for a device
    pub fn new_device<I, A, D>(
        instance_id: I,
        app_id: A,
        device_id: D,
        paths: Vec<String>,
    ) -> Vec<Event>
    where
        I: ToString,
        A: ToString,
        D: ToString,
    {
        Self::new_change(paths, |path| Event::Device {
            instance: instance_id.to_string(),
            application: app_id.to_string(),
            id: device_id.to_string(),
            path,
        })
    }
}

#[derive(Debug, Error)]
pub enum EventSenderError<E>
where
    E: std::error::Error + std::fmt::Debug + 'static,
{
    #[error("Failed to send the event")]
    Sender(#[from] E),
    #[error("Failed to process event")]
    Event(EventError),
    #[error("Cloud event error")]
    CloudEvent(cloudevents::message::Error),
}

#[derive(Clone, Debug, Error)]
pub enum EventError {
    #[error("Failed to parse event: {0}")]
    Parse(String),
    #[error("Failed to build event: {0}")]
    Builder(cloudevents::event::EventBuilderError),
    #[error("Unknown event type: {0}")]
    UnknownType(String),
}

type SenderResult<T, E> = Result<T, EventSenderError<E>>;

#[async_trait]
pub trait EventSender: Clone + Send + Sync {
    type Error: std::error::Error + std::fmt::Debug + 'static;

    async fn notify<I>(&self, events: I) -> SenderResult<(), Self::Error>
    where
        I: IntoIterator<Item = Event> + Sync + Send;
}

impl TryInto<cloudevents::Event> for Event {
    type Error = EventError;

    fn try_into(self) -> Result<cloudevents::Event, Self::Error> {
        let builder = cloudevents::event::EventBuilderV10::new()
            .id(uuid::Uuid::new_v4().to_string())
            .time(Utc::now());

        let builder = match self {
            Self::Application { instance, id, path } => builder
                .ty(EVENT_TYPE_APPLICATION)
                .source(format!("drogue:/{}/{}", instance, id))
                .subject(path)
                .extension(EXT_PARTITIONKEY, format!("{}/{}", instance, id))
                .extension(EXT_INSTANCE, instance)
                .extension(EXT_APPLICATION, id),
            Self::Device {
                instance,
                application,
                id,
                path,
            } => builder
                .ty(EVENT_TYPE_DEVICE)
                .source(format!("drogue:/{}/{}/{}", instance, application, id))
                .subject(path)
                .extension(
                    EXT_PARTITIONKEY,
                    format!("{}/{}/{}", instance, application, id),
                )
                .extension(EXT_INSTANCE, instance)
                .extension(EXT_APPLICATION, application)
                .extension(EXT_DEVICE, id),
        };

        builder.build().map_err(EventError::Builder)
    }
}

impl TryInto<Event> for cloudevents::Event {
    type Error = EventError;

    fn try_into(self) -> Result<Event, Self::Error> {
        match self.ty() {
            EVENT_TYPE_APPLICATION => Event::from_app(self),
            EVENT_TYPE_DEVICE => Event::from_device(self),
            ty => Err(EventError::UnknownType(ty.into())),
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use anyhow::Context;
    use std::convert::TryInto;
    use uuid::Uuid;

    #[test]
    fn test_encode() -> anyhow::Result<()> {
        let ce: cloudevents::Event = Event::Application {
            instance: "instance".to_string(),
            id: "application".to_string(),
            path: ".spec.core".to_string(),
        }
        .try_into()?;

        assert_eq!(
            ce,
            cloudevents::EventBuilderV10::new()
                .ty(EVENT_TYPE_APPLICATION)
                .id(ce.id())
                .source("drogue:/instance/application")
                .subject(".spec.core")
                .time(*ce.time().unwrap())
                .extension(EXT_PARTITIONKEY, "instance/application")
                .extension(EXT_INSTANCE, "instance")
                .extension(EXT_APPLICATION, "application")
                .build()?
        );

        Ok(())
    }

    #[test]
    fn test_decode() -> anyhow::Result<()> {
        let ce = cloudevents::EventBuilderV10::new()
            .ty(EVENT_TYPE_APPLICATION)
            .id(Uuid::new_v4().to_string())
            .source("drogue:/instance/application")
            .subject(".spec.credentials")
            .extension(EXT_PARTITIONKEY, "application")
            .extension(EXT_INSTANCE, "instance")
            .extension(EXT_APPLICATION, "application")
            .extension(EXT_DEVICE, "device")
            .build()
            .context("Failed to build CloudEvent")?;

        let event = ce.try_into()?;

        assert_eq!(
            Event::Application {
                instance: "instance".to_string(),
                id: "application".to_string(),
                path: ".spec.credentials".to_string()
            },
            event
        );

        Ok(())
    }
}
