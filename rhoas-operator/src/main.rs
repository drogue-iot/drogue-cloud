use drogue_cloud_rhoas_operator::{run, Config};
use drogue_cloud_service_common::app;

#[actix::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
