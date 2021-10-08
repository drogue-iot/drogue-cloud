use dotenv::dotenv;
use drogue_cloud_device_management_service::{run, Config};
use drogue_cloud_service_common::config::ConfigFromEnv;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();
    // Initialize config from environment variables
    let config = Config::from_env().unwrap();

    run(config).await
}
