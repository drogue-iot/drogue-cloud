use async_trait::async_trait;

#[async_trait]
pub trait EventHandler {
    type Event;
    type Error;

    async fn handle(&self, event: &Self::Event) -> Result<(), Self::Error>;
}
