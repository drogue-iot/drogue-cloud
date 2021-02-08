use actix_cors::Cors;
use actix_web::{middleware::Condition, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use dotenv::dotenv;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self, ManagementServiceConfig, PostgresManagementService},
    Config, WebData,
};
use drogue_cloud_service_common::endpoints::create_endpoint_source;
use drogue_cloud_service_common::openid::{
    create_client, AuthConfig, Authenticator, AuthenticatorError,
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

    // OpenIdConnect
    let enable_auth = config.enable_auth;

    let (client, scopes) = if enable_auth {
        let config: AuthConfig = AuthConfig::init_from_env()?;
        (
            Some(create_client(&config, endpoints).await?),
            config.scopes,
        )
    } else {
        (None, "".into())
    };

    let data = web::Data::new(WebData {
        authenticator: Authenticator { client, scopes },
        service: service::PostgresManagementService::new(ManagementServiceConfig::from_env()?)?,
    });

    let max_json_payload_size = 64 * 1024;

    HttpServer::new(move || app!(data, enable_auth, max_json_payload_size))
        .bind(config.bind_addr)?
        .run()
        .await?;

    Ok(())
}
