mod processor;

pub use processor::*;

use async_trait::async_trait;
use drogue_cloud_registry_events::stream::EventHandler;
use std::boxed::Box;

#[async_trait]
impl<E> EventHandler for EventDispatcher<E>
where
    E: Send + Sync,
{
    type Event = E;
    type Error = ();

    async fn handle(&self, event: &Self::Event) -> Result<(), Self::Error> {
        for processor in &self.processors {
            if processor.handle(event).await? {
                return Ok(());
            }
        }

        Ok(())
    }
}

/// Dispatch events to the different [`EventHandler`]s.
pub struct EventDispatcher<E> {
    processors: Vec<Box<dyn EventProcessor<E>>>,
}

impl<E> EventDispatcher<E> {
    /// Create a new instance for a list of events handlers.
    pub fn new(processors: Vec<Box<dyn EventProcessor<E>>>) -> Self {
        Self { processors }
    }

    /// Create a new instance for a single handler.
    pub fn one<P>(processor: P) -> Self
    where
        P: EventProcessor<E> + 'static,
    {
        Self::new(vec![Box::new(processor)])
    }
}
