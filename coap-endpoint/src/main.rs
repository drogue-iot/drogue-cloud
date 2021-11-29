use drogue_cloud_coap_endpoint::{run, Config};
use drogue_cloud_service_common::app;
use tokio::signal;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    futures::try_join!(app!(), signal::ctrl_c().await)
}
