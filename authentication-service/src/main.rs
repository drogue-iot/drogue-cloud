use actix_web::{web, App, HttpServer};
use dotenv::dotenv;
use drogue_cloud_authentication_service::{
    endpoints,
    service::{self, AuthenticationServiceConfig},
    Config, WebData,
};
use drogue_cloud_service_common::openid::{
    create_client, AuthConfig, Authenticator, ConfigFromEnv,
};
use envconfig::Envconfig;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::init_from_env().unwrap();
    let data = WebData {
        service: service::PostgresAuthenticationService::new(
            AuthenticationServiceConfig::from_env()?,
        )?,
    };

    let max_json_payload_size = config.max_json_payload_size;

    let config: AuthConfig = AuthConfig::init_from_env()?;
    let scopes = config.scopes;
    let client = Some(create_client(&config).await?);

    let authenticator = web::Data::new(Authenticator::new(client, scopes).await);

    HttpServer::new(move || {
        drogue_cloud_authentication_service::app!(data, max_json_payload_size, authenticator)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
