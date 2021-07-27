use anyhow::bail;
use async_trait::async_trait;
use drogue_cloud_registry_events::stream::EventHandler;
use drogue_cloud_service_api::health::{HealthCheckError, HealthChecked};
use futures::{stream, Stream, StreamExt, TryStreamExt};
use kube::api::DynamicObject;
use kube_runtime::watcher::Event;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Run a stream to completion, sending items to the handler
pub trait RunStream {
    fn run_stream<H>(self, handler: H) -> Runner
    where
        H: EventHandler<Event = DynamicObject> + Send + Sync + 'static;
}

impl<S, E> RunStream for S
where
    E: Send,
    S: Stream<Item = Result<Event<DynamicObject>, E>> + Send + 'static,
{
    fn run_stream<H>(self, handler: H) -> Runner
    where
        H: EventHandler<Event = DynamicObject> + Send + Sync + 'static,
    {
        Runner::new(Box::pin(self), handler)
    }
}

pub struct Runner {
    _handle: JoinHandle<Result<(), anyhow::Error>>,
    running: Arc<AtomicBool>,
}

impl Runner {
    pub fn new<S, H, E>(stream: S, handler: H) -> Self
    where
        E: Send,
        S: Stream<Item = Result<Event<DynamicObject>, E>> + Unpin + Send + 'static,
        H: EventHandler<Event = DynamicObject> + Send + Sync + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        let handle = tokio::spawn(async move {
            // expand resources from events
            let mut stream = stream.map_ok(Self::expand::<E>).try_flatten();
            // handle events
            while let Some(Ok(event)) = stream.next().await {
                log::debug!("Processing event: {:?}", event);
                let mut cnt = 0;
                while handler.handle(&event).await.is_err() {
                    if cnt > 10 {
                        bail!("Failed to process event");
                    } else {
                        cnt += 1;
                    }
                }
            }
            r.store(false, Ordering::Relaxed);
            Ok::<_, anyhow::Error>(())
        });
        Self {
            _handle: handle,
            running,
        }
    }

    fn expand<E>(event: Event<DynamicObject>) -> impl Stream<Item = Result<DynamicObject, E>> {
        match event {
            Event::Applied(resource) | Event::Deleted(resource) => stream::iter(vec![resource]),
            Event::Restarted(resources) => stream::iter(resources),
        }
        .map(Ok)
    }
}

#[async_trait]
impl HealthChecked for Runner {
    async fn is_alive(&self) -> Result<(), HealthCheckError> {
        if !self.running.load(Ordering::Relaxed) {
            HealthCheckError::nok("Event loop not running")
        } else {
            Ok(())
        }
    }
}
