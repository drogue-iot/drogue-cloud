use drogue_cloud_service_common::app;
use drogue_cloud_websocket_integration::{run, Config};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
