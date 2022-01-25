use drogue_cloud_service_common::app;
use drogue_cloud_websocket_integration::{run, Config};

#[drogue_cloud_service_api::webapp::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
