use dotenv::dotenv;
use drogue_cloud_service_common::config::ConfigFromEnv;
use drogue_cloud_topic_operator::{run, Config};

#[actix::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env().unwrap();

    run(config).await
}
