use actix_web::{web, App, HttpServer};
use dotenv::dotenv;
use drogue_cloud_authentication_service::{
    service::{self, AuthenticationServiceConfig},
    Config, WebData,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    endpoints::create_endpoint_source,
    openid::{create_client, AuthConfig, Authenticator},
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

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;

    // extract required endpoint information
    let endpoints = endpoint_source.eval_endpoints().await?;

    let max_json_payload_size = config.max_json_payload_size;

    let auth_config: AuthConfig = AuthConfig::init_from_env()?;
    let scopes = auth_config.scopes;
    let client = Some(create_client(&auth_config, endpoints).await?);

    let authenticator = web::Data::new(Authenticator::new(client, scopes).await);

    HttpServer::new(move || {
        drogue_cloud_authentication_service::app!(data, max_json_payload_size, authenticator)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
