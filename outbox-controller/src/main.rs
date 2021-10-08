use dotenv::dotenv;
use drogue_cloud_outbox_controller::{run, Config};
use drogue_cloud_service_common::config::ConfigFromEnv;

#[actix::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env()?;

    run(config).await
}
