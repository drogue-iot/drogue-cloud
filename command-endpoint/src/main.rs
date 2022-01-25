use drogue_cloud_command_endpoint::{run, Config};
use drogue_cloud_service_common::app;

#[drogue_cloud_service_api::webapp::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
