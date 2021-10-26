use drogue_cloud_mqtt_integration::{run, Config};
use drogue_cloud_service_common::app;

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
