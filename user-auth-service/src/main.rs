use actix_web::{web, App, HttpServer};
use dotenv::dotenv;
use drogue_cloud_api_key_service::{
    endpoints::WebData as KeycloakWebData, service::KeycloakApiKeyService,
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv, health::HealthServer, openid::Authenticator, openid_auth,
};
use drogue_cloud_user_auth_service::{endpoints, service, Config, WebData};
use futures::TryFutureExt;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env()?;

    let max_json_payload_size = config.max_json_payload_size;
    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let data = web::Data::new(WebData {
        authenticator,
        service: service::PostgresAuthorizationService::new(config.service)?,
    });

    let api_key = web::Data::new(KeycloakWebData {
        service: KeycloakApiKeyService::new(config.keycloak)?,
    });

    // health server

    let health = HealthServer::new(config.health, vec![Box::new(data.service.clone())]);

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthorizationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        drogue_cloud_user_auth_service::app!(
            data,
            KeycloakApiKeyService,
            api_key,
            max_json_payload_size,
            enable_auth,
            auth
        )
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
