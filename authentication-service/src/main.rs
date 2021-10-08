use dotenv::dotenv;
use drogue_cloud_authentication_service::{run, service::AuthenticationServiceConfig, Config};
use drogue_cloud_service_common::config::ConfigFromEnv;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env()
        .map(|mut c| {
            c.auth_service_config = AuthenticationServiceConfig::from_env().unwrap();
            c
        })
        .unwrap();

    run(config).await
}
