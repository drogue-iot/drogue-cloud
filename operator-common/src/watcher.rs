use anyhow::bail;
use async_trait::async_trait;
use drogue_cloud_registry_events::stream::EventHandler;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use kube::api::DynamicObject;
use kube_runtime::watcher::Event;

/// Run a stream to completion, sending items to the handler
#[async_trait]
pub trait RunStream {
    type Error;

    async fn run_stream<H>(self, handler: H) -> Result<(), Self::Error>
    where
        H: EventHandler<Event = DynamicObject> + Send + Sync + 'static;
}

#[async_trait]
impl<S, E> RunStream for S
where
    E: std::error::Error + Send + Sync + 'static,
    S: Stream<Item = Result<Event<DynamicObject>, E>> + Send + 'static,
{
    type Error = anyhow::Error;

    async fn run_stream<H>(self, handler: H) -> Result<(), Self::Error>
    where
        H: EventHandler<Event = DynamicObject> + Send + Sync + 'static,
    {
        let stream = Box::pin(self);
        // expand resources from events
        let mut stream = stream
            .map_err(anyhow::Error::from)
            .map_ok(|event| {
                match event {
                    Event::Applied(resource) | Event::Deleted(resource) => {
                        stream::iter(vec![resource])
                    }
                    Event::Restarted(resources) => stream::iter(resources),
                }
                .map(Result::<_, anyhow::Error>::Ok)
            })
            .try_flatten();
        // handle events
        while let Some(event) = stream.try_next().await? {
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
        bail!("Stream must not end")
    }
}
