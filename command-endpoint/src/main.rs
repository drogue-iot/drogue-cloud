use actix_web::{
    get, http::header, middleware, post, web, App, HttpResponse, HttpServer, Responder,
};
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    downstream::{self, DownstreamSender},
    error::HttpEndpointError,
};
use envconfig::Envconfig;
use serde::Deserialize;
use serde_json::json;

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "ENABLE_AUTH", default = "false")]
    pub enable_auth: bool,
}

#[derive(Deserialize)]
pub struct CommandOptions {
    pub application: String,
    pub device: String,

    pub command: String,
    pub timeout: Option<u64>,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[post("/command")]
async fn command(
    endpoint: web::Data<DownstreamSender>,
    web::Query(opts): web::Query<CommandOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    log::info!("Send command '{}' to '{}' / '{}'",opts.command, opts.application, opts.device);

    endpoint
        .publish_http_default(
            downstream::Publish {
                channel: opts.command,
                app_id: opts.application,
                device_id: opts.device,
                options: downstream::PublishOptions {
                    topic: None,
                    content_type: req
                        .headers()
                        .get(header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string()),
                    ..Default::default()
                },
            },
            body,
        )
        .await
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
            .service(health)
            .service(index)
            .service(command)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
