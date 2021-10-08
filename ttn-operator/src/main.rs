use dotenv::dotenv;
use drogue_cloud_service_common::{client::RegistryConfig, config::ConfigFromEnv};
use drogue_cloud_ttn_operator::{run, Config};

#[actix::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let mut config = Config::from_env().unwrap();
    config.registry = Some(RegistryConfig::from_env().unwrap());

    run(config).await
}
