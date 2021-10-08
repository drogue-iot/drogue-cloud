use dotenv::dotenv;
use drogue_cloud_mqtt_integration::{run, Config};
use drogue_cloud_service_common::config::ConfigFromEnv;

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::from_env().unwrap();

    run(config).await
}
