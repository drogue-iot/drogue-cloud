use drogue_cloud_registry_events::Event;

pub struct Controller {}

impl Controller {
    pub async fn handle_event(&self, event: Event) -> Result<(), anyhow::Error> {
        log::info!("Event: {:#?}", event);

        Ok(())
    }
}
