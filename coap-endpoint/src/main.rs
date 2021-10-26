use drogue_cloud_coap_endpoint::{run, Config};
use drogue_cloud_service_common::app;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
