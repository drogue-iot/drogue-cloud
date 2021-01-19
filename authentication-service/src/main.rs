use actix_web::{web, App, HttpServer};
use dotenv::dotenv;
use drogue_cloud_authentication_service::{
    endpoints,
    service::{self, AuthenticationServiceConfig},
    Config, WebData,
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

    HttpServer::new(move || drogue_cloud_authentication_service::app!(data, max_json_payload_size))
        .bind(config.bind_addr)?
        .run()
        .await?;

    Ok(())
}
