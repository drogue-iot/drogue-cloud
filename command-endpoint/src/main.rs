use dotenv::dotenv;
use drogue_cloud_command_endpoint::{run, Config};
use drogue_cloud_service_common::{
    client::{RegistryConfig, UserAuthClientConfig},
    config::ConfigFromEnv,
};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let mut config = Config::from_env()?;
    config.user_auth = Some(UserAuthClientConfig::from_env()?);
    config.registry = Some(RegistryConfig::from_env()?);

    run(config).await
}
