use crate::controller::base::{BaseController, ControllerOperation, Key};
use async_std::sync::Mutex;
use async_trait::async_trait;
use drogue_cloud_registry_events::Event;
use std::boxed::Box;

#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: &Event) -> Result<bool, ()>;
}

#[cfg(feature = "with_actix")]
pub mod actix {
    use super::*;
    use actix_web::{post, web, HttpResponse};
    use serde_json::json;
    use std::convert::TryInto;

    #[post("/")]
    pub async fn events(
        event: cloudevents::Event,
        data: web::Data<EventSource>,
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

pub struct EventSource {
    processors: Vec<Box<dyn EventHandler>>,
}

impl EventSource {
    pub fn new(processors: Vec<Box<dyn EventHandler>>) -> Self {
        Self { processors }
    }

    pub fn one<H>(handler: H) -> Self
    where
        H: EventHandler + 'static,
    {
        Self::new(vec![Box::new(handler)])
    }

    /// Handle the event.
    pub async fn handle(&self, event: &Event) -> Result<(), ()> {
        for processor in &self.processors {
            if processor.handle(event).await? {
                return Ok(());
            }
        }

        Ok(())
    }
}

#[async_trait]
pub trait EventProcessor {
    type Key;

    /// Translate into key, or nothing.
    fn is_relevant(&self, event: &Event) -> Option<Self::Key>;

    /// Process the event
    async fn process(&self, key: Self::Key) -> Result<(), ()>;
}

#[async_trait]
impl<P> EventHandler for P
where
    P: EventProcessor + Send + Sync,
    P::Key: Send + Sync,
{
    async fn handle(&self, event: &Event) -> Result<bool, ()> {
        if let Some(key) = self.is_relevant(&event) {
            self.process(key).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub struct FnEventProcessor<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
    O: ControllerOperation<K, RI, RO> + Send + Sync,
{
    base_controller: Mutex<BaseController<K, RI, RO, O>>,
    f: Box<dyn Fn(&Event) -> Option<K> + Send + Sync>,
}

impl<K, RI, RO, O> FnEventProcessor<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync,
    RO: Clone + Send + Sync,
    O: ControllerOperation<K, RI, RO> + Send + Sync,
{
    pub fn new<F>(base_controller: Mutex<BaseController<K, RI, RO, O>>, f: F) -> Self
    where
        F: Fn(&Event) -> Option<K> + Send + Sync + 'static,
    {
        Self {
            base_controller,
            f: Box::new(f),
        }
    }
}

#[async_trait]
impl<K, RI, RO, O> EventProcessor for FnEventProcessor<K, RI, RO, O>
where
    K: Key,
    RI: Clone + Send + Sync + 'static,
    RO: Clone + Send + Sync + 'static,
    O: ControllerOperation<K, RI, RO> + Send + Sync + 'static,
{
    type Key = K;

    fn is_relevant(&self, event: &Event) -> Option<Self::Key> {
        (self.f)(event)
    }

    async fn process(&self, key: Self::Key) -> Result<(), ()> {
        self.base_controller.lock().await.process(key).await
    }
}
