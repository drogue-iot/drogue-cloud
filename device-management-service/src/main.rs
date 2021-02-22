use actix_cors::Cors;
use actix_web::{middleware::Condition, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use anyhow::Context;
use dotenv::dotenv;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self, PostgresManagementService, PostgresManagementServiceConfig},
    Config, WebData,
};
use drogue_cloud_registry_events::reqwest::ReqwestEventSender;
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    endpoints::create_endpoint_source,
    openid::{create_client, AuthConfig, Authenticator, AuthenticatorError},
};
use envconfig::Envconfig;
use url::Url;

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

    let sender = ReqwestEventSender::new(
        reqwest::ClientBuilder::new()
            .build()
            .context("Failed to create event sender client")?,
        Url::parse(&config.event_url).context("Failed to parse 'EVENT_URL'")?,
    );

    let data = web::Data::new(WebData {
        authenticator: Authenticator { client, scopes },
        service: service::PostgresManagementService::new(
            PostgresManagementServiceConfig::from_env()?,
            sender,
        )?,
    });

    let max_json_payload_size = 64 * 1024;

    HttpServer::new(move || app!(ReqwestEventSender, data, enable_auth, max_json_payload_size))
        .bind(config.bind_addr)?
        .run()
        .await?;

    Ok(())
}
