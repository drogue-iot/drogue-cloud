use dotenv::dotenv;
use drogue_cloud_authentication_service::{run, service::AuthenticationServiceConfig, Config};
use drogue_cloud_service_common::config::ConfigFromEnv;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let mut config = Config::from_env()?;
    config.auth_service_config = Some(AuthenticationServiceConfig::from_env()?);

    run(config).await
}
