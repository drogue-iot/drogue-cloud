use actix_web::{web, App, HttpServer};
use anyhow::Context;
use dotenv::dotenv;
use drogue_cloud_authentication_service::{
    endpoints, health_checks,
    service::{self, AuthenticationServiceConfig},
    Config, WebData,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv, health::HealthServer, openid::Authenticator, openid_auth,
};
use futures::TryFutureExt;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env().context("Failed to read configuration")?;

    println!("Config: {:#?}", config);

    let max_json_payload_size = config.max_json_payload_size;
    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let data = web::Data::new(WebData {
        authenticator,
        service: service::PostgresAuthenticationService::new(
            AuthenticationServiceConfig::from_env()?,
        )?,
    });

    // health server

    let health = HealthServer::new(config.health, health_checks(data.service.clone()));

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthenticationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        drogue_cloud_authentication_service::app!(data, max_json_payload_size, enable_auth, auth)
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
