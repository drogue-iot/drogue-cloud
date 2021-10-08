use dotenv::dotenv;
use drogue_cloud_device_management_service::{
    run, service::PostgresManagementServiceConfig, Config,
};
use drogue_cloud_service_common::config::ConfigFromEnv;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();
    // Initialize config from environment variables
    let mut config = Config::from_env()?;
    config.database_config = Some(PostgresManagementServiceConfig::from_env()?);

    run(config).await
}
