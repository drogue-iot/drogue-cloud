mod endpoints;
mod service;

use crate::service::{OutboxService, OutboxServiceConfig};
use actix_web::{
    get, middleware,
    web::{self},
    App, HttpResponse, HttpServer, Responder,
};
use dotenv::dotenv;
use drogue_cloud_service_common::config::ConfigFromEnv;
use envconfig::Envconfig;
use serde_json::json;

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
}

pub struct WebData {
    pub service: OutboxService,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    let config = Config::init_from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let data = web::Data::new(WebData {
        service: service::OutboxService::new(OutboxServiceConfig::from_env()?)?,
    });

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(data.clone())
            .service(endpoints::health)
            .service(index)
            .service(endpoints::events)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
