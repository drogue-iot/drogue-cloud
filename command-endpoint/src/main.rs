use actix_web::{
    get,
    http::header,
    middleware::{self, Condition},
    web, App, HttpResponse, HttpServer, Responder,
};
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    downstream::{self, DownstreamSender},
    error::HttpEndpointError,
};
use drogue_cloud_service_common::{client::RegistryClient, openid::Authenticator, openid_auth};
use envconfig::Envconfig;
use serde::Deserialize;
use serde_json::json;
use url::Url;

use actix_web_httpauth::extractors::bearer::BearerAuth;

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "ENABLE_AUTH", default = "true")]
    pub enable_auth: bool,
    #[envconfig(from = "REGISTRY_SERVICE_URL", default = "http://registry:8080")]
    pub registry_service_url: String,
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

async fn command(
    endpoint: web::Data<DownstreamSender>,
    web::Query(opts): web::Query<CommandOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    registry: web::Data<RegistryClient>,
    token: BearerAuth,
) -> Result<HttpResponse, HttpEndpointError> {
    log::info!(
        "Send command '{}' to '{}' / '{}'",
        opts.command,
        opts.application,
        opts.device
    );
    let response = registry
        .get_device(&opts.application, &opts.device, token.token())
        .await;

    match response {
        Ok(device) => {
            log::debug!("Found device {:?}", device);
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
        Err(err) => {
            log::info!("Error {:?}", err);
            Ok(HttpResponse::NotAcceptable().finish())
        }
    }
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting Command service endpoint");

    let sender = DownstreamSender::new()?;

    let config = Config::init_from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let data = web::Data::new(WebData { authenticator });

    let registry = RegistryClient::new(
        Default::default(),
        Url::parse(&config.registry_service_url)?,
    );

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
            .data(registry.clone())
            .service(health)
            .service(index)
            .service(
                web::resource("/command")
                    .wrap(Condition::new(enable_auth, auth))
                    .route(web::post().to(command)),
            )
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
