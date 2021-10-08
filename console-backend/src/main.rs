use anyhow::Context;
use drogue_cloud_console_backend::{run, Config};
use drogue_cloud_service_common::{
    client::{RegistryConfig, UserAuthClientConfig},
    config::ConfigFromEnv,
    endpoints::create_endpoint_source,
    openid::TokenConfig,
};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut config = Config::from_env()?;

    config.user_auth = Some(UserAuthClientConfig::from_env()?);
    config.registry = Some(RegistryConfig::from_env()?);
    config.console_token_config = TokenConfig::from_env_prefix("UI")
        .context("Failed to find console token config")?
        .amend_with_env();

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;
    log::info!("Using endpoint source: {:?}", endpoint_source);
    let endpoints = endpoint_source.eval_endpoints().await?;

    run(config, endpoints).await
}
