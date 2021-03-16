use actix_web::{
    get,
    http::header,
    middleware::{self, Condition},
    post, web, App, HttpResponse, HttpServer, Responder,
};
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    downstream::{self, DownstreamSender},
    error::HttpEndpointError,
};
use drogue_cloud_service_common::openid::Authenticator;
use drogue_cloud_service_common::{
    endpoints::create_endpoint_source,
    openid::{create_client, AuthenticatorConfig},
    openid_auth,
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
    #[envconfig(from = "ENABLE_AUTH", default = "true")]
    pub enable_auth: bool,
}

#[derive(Deserialize)]
pub struct CommandOptions {
    pub application: String,
    pub device: String,

    pub command: String,
    pub timeout: Option<u64>,
}

#[derive(Debug)]
pub struct WebData {
    pub authenticator: Option<Authenticator>,
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
    log::info!(
        "Send command '{}' to '{}' / '{}'",
        opts.command,
        opts.application,
        opts.device
    );

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

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;

    // extract required endpoint information
    let endpoints = endpoint_source.eval_endpoints().await?;

    let enable_auth = config.enable_auth;

    let client = if enable_auth {
        let config = AuthenticatorConfig::init_from_env()?;
        Some(create_client(&config, endpoints).await?)
    } else {
        None
    };

    let data = web::Data::new(WebData {
        authenticator: Some(Authenticator::new(client).await),
    });

    HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData>>()
            .as_ref()
            .and_then(|d|d.authenticator.as_ref())
        });
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(data.clone())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(sender.clone())
            .service(health)
            .service(index)
            .service(
                web::scope("/")
                    .wrap(Condition::new(enable_auth, auth))
                    .service(command),
            )
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
