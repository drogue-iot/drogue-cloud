use dotenv::dotenv;
use drogue_cloud_mqtt_integration::{run, Config};
use drogue_cloud_service_common::{
    client::{RegistryConfig, UserAuthClientConfig},
    config::ConfigFromEnv,
};

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let mut config = Config::from_env()?;
    config.user_auth = Some(UserAuthClientConfig::from_env()?);
    config.registry = Some(RegistryConfig::from_env()?);

    run(config).await
}
