use drogue_cloud_outbox_controller::{run, Config};
use drogue_cloud_service_common::app;

#[actix::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
