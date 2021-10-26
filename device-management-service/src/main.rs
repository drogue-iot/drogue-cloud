use drogue_cloud_device_management_service::{run, Config};
use drogue_cloud_service_common::app;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
