mod command;
mod downstream;
mod telemetry;
mod ttn;

use actix_web::web::Data;
use actix_web::{get, middleware, post, web, App, HttpResponse, HttpServer, Responder};
use cloudevents_sdk_actix_web::HttpRequestExt;
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    auth::{AuthConfig, DeviceAuthenticator},
    command_router::CommandRouter,
    downstream::DownstreamSender,
};
use envconfig::Envconfig;
use serde_json::json;
use std::convert::TryInto;

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "MAX_PAYLOAD_SIZE", default = "65536")]
    pub max_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:8081")]
    pub health_bind_addr: String,
    #[envconfig(from = "AUTH_SERVICE_URL")]
    pub auth_service_url: Option<String>,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[post("/command-service")]
async fn command_service(
    body: web::Bytes,
    req: web::HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Req: {:?}", req);

    let mut request_event = req.to_event(payload).await?;
    request_event.set_data(
        "application/json",
        String::from_utf8(body.as_ref().to_vec()).unwrap(),
    );

    if let Err(e) = CommandRouter::send(request_event).await {
        log::error!("Failed to route command: {}", e);
        HttpResponse::BadRequest().await
    } else {
        HttpResponse::Ok().await
    }
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting HTTP service endpoint");

    let sender = DownstreamSender::new()?;

    let config = Config::init_from_env()?;
    let max_payload_size = config.max_payload_size;
    let max_json_payload_size = config.max_json_payload_size;

    let authenticator: DeviceAuthenticator = AuthConfig::init_from_env()?.try_into()?;

    HttpServer::new(move || {
        let app = App::new()
            .wrap(middleware::Logger::default())
            .app_data(web::PayloadConfig::new(max_payload_size))
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(sender.clone());

        let app = app.app_data(Data::new(authenticator.clone()));

        app.service(index)
            // the standard endpoint
            .service(
                web::scope("/v1")
                    .service(telemetry::publish_plain)
                    .service(telemetry::publish_tail),
            )
            // The Things Network variant
            .service(web::scope("/ttn").service(ttn::publish))
            .service(command_service)
            //fixme : bind to a different port
            .service(health)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    // fixme
    //
    // let health_server = HttpServer::new(move || App::new().service(health))
    //     .bind(config.health_bind_addr)?
    //     .run();
    //
    // future::try_join(app_server, health_server).await?;

    Ok(())
}
