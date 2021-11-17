use drogue_cloud_service_common::app;
use drogue_cloud_topic_admin_operator::{run, Config};

#[actix::main]
async fn main() -> anyhow::Result<()> {
    app!();
}
