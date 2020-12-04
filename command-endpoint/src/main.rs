use actix_web::{
    get, http::header, middleware, post, web, App, HttpResponse, HttpServer, Responder,
};

use drogue_cloud_endpoint_common::downstream::{
    DownstreamSender, Publish,
};

use drogue_cloud_endpoint_common::error::HttpEndpointError;
use serde::Deserialize;
use serde_json::json;

use dotenv::dotenv;
use envconfig::Envconfig;

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "ENABLE_AUTH", default = "false")]
    pub enable_auth: bool,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[derive(Deserialize)]
pub struct PublishOptions {
    model_id: Option<String>,
}

#[post("/command/{device_id}/{channel}")]
async fn publish(
    endpoint: web::Data<DownstreamSender>,
    web::Path((device_id, channel)): web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    log::info!("Published to '{}'", channel);

    endpoint.publish_http(
        Publish {
            channel,
            device_id,
            model_id: opts.model_id,
            content_type: req
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string()),
        },
        body,
    ).await
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting Command service endpoint");

    let sender = DownstreamSender::new()?;

    let config = Config::init_from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    HttpServer::new(move || {

        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(sender.clone())
            .service(index)
            .service(publish)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
