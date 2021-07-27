mod processor;

pub use processor::*;

use async_trait::async_trait;
use drogue_cloud_registry_events::{stream::EventHandler, Event};
use std::boxed::Box;

#[cfg(feature = "with_actix")]
pub mod actix {
    use super::*;
    use actix_web::{post, web, HttpResponse};
    use serde_json::json;
    use std::convert::TryInto;

    #[post("/")]
    pub async fn events(
        event: cloudevents::Event,
        data: web::Data<EventDispatcher<Event>>,
    ) -> Result<HttpResponse, actix_web::error::Error> {
        log::debug!("Received event: {:?}", event);

        let event: Event = match event.try_into() {
            Ok(event) => event,
            Err(err) => {
                return Ok(HttpResponse::BadRequest().json(json!({ "details": format!("{}", err) })))
            }
        };

        log::debug!("Registry event: {:?}", event);

        Ok(match data.handle(&event).await {
            Ok(_) => HttpResponse::Ok().finish(),
            Err(_) => HttpResponse::InternalServerError().finish(),
        })
    }
}

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
