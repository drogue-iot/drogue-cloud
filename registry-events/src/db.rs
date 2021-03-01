use crate::{Event, EventSender, SenderResult};
use async_trait::async_trait;
use drogue_cloud_database_common::{
    error::ServiceError,
    models::outbox::{OutboxAccessor, OutboxEntry},
};

impl From<Event> for OutboxEntry {
    fn from(e: Event) -> Self {
        match e {
            Event::Application {
                instance,
                id,
                path,
                generation,
            } => OutboxEntry {
                instance_id: instance,
                app_id: id,
                device_id: None,
                path,
                generation,
            },
            Event::Device {
                instance,
                application,
                id,
                path,
                generation,
            } => OutboxEntry {
                instance_id: instance,
                app_id: application,
                device_id: Some(id),
                path,
                generation,
            },
        }
    }
}

impl From<OutboxEntry> for Event {
    fn from(entry: OutboxEntry) -> Self {
        if let Some(device_id) = entry.device_id {
            Event::Device {
                instance: entry.instance_id,
                application: entry.app_id,
                id: device_id,
                path: entry.path,
                generation: entry.generation,
            }
        } else {
            Event::Application {
                instance: entry.instance_id,
                id: entry.app_id,
                path: entry.path,
                generation: entry.generation,
            }
        }
    }
}

#[async_trait]
impl<A> EventSender for A
where
    A: OutboxAccessor + Send + Sync,
{
    type Error = ServiceError;

    async fn notify<I>(&self, events: I) -> SenderResult<(), Self::Error>
    where
        I: IntoIterator<Item = Event> + Sync + Send,
        I::IntoIter: Sync + Send,
    {
        for e in events.into_iter() {
            self.create(e.into()).await?;
        }

        Ok(())
    }
}
