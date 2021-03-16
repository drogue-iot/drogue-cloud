use actix_web::{web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use dotenv::dotenv;
use drogue_cloud_authentication_service::{
    endpoints,
    service::{self, AuthenticationServiceConfig},
    Config, WebData,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    endpoints::create_endpoint_source,
    openid::{create_client, AuthConfig, Authenticator},
    openid_auth,
};
use envconfig::Envconfig;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::init_from_env().unwrap();

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;

    // extract required endpoint information
    let endpoints = endpoint_source.eval_endpoints().await?;

    let max_json_payload_size = config.max_json_payload_size;

    let auth_config: AuthConfig = AuthConfig::init_from_env()?;
    let client = Some(create_client(&auth_config, endpoints).await?);

    let data = web::Data::new(WebData {
        authenticator: Some(Authenticator::new(client).await),
        service: service::PostgresAuthenticationService::new(
            AuthenticationServiceConfig::from_env()?,
        )?,
    });

    HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthenticationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        drogue_cloud_authentication_service::app!(data, max_json_payload_size, auth)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
