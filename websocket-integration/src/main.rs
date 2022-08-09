use drogue_cloud_service_api::PROJECT;
use drogue_cloud_service_common::runtime;
use drogue_cloud_websocket_integration::run;

// We still need actix::main here as we use actix-actors.
#[drogue_cloud_service_api::webapp::main]
async fn main() -> anyhow::Result<()> {
    runtime!(PROJECT).exec(run).await
}
