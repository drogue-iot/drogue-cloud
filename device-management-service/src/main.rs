use actix_cors::Cors;
use actix_web::{middleware::Condition, web, App, HttpServer};
use anyhow::Context;
use dotenv::dotenv;
use drogue_cloud_admin_service::apps;
use drogue_cloud_device_management_service::{
    app, endpoints,
    service::{self, PostgresManagementServiceConfig},
    Config, WebData,
};
use drogue_cloud_registry_events::sender::KafkaEventSender;
use drogue_cloud_service_common::{
    config::ConfigFromEnv, health::HealthServer, openid::Authenticator, openid_auth,
};
use futures::TryFutureExt;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env().unwrap();

    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let sender = KafkaEventSender::new(config.kafka_sender)
        .context("Unable to create Kafka event sender")?;

    let max_json_payload_size = 64 * 1024;

    let service = service::PostgresManagementService::new(
        PostgresManagementServiceConfig::from_env()?,
        sender,
    )?;

    let data = web::Data::new(WebData {
        authenticator,
        service: service.clone(),
    });

    // health server

    let health = HealthServer::new(config.health, vec![Box::new(service.clone())]);

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresManagementService<KafkaEventSender>>>>()
            .as_ref()
            .and_then(|d|d.authenticator.as_ref())
        });
        app!(KafkaEventSender, enable_auth, max_json_payload_size, auth)
            // for the management service
            .app_data(data.clone())
            // for the admin service
            .app_data(web::Data::new(apps::WebData {
                service: service.clone(),
            }))
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
