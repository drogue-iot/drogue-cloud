use actix_web::{
    get,
    http::header,
    middleware::{self, Condition},
    web, App, HttpResponse, HttpServer, Responder,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use dotenv::dotenv;
use drogue_client::{registry, Context};
use drogue_cloud_endpoint_common::{downstream::DownstreamSender, error::HttpEndpointError};
use drogue_cloud_integration_common::{self, commands::CommandOptions};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::Authenticator,
    openid_auth,
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use std::str;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,
    #[serde(default = "registry_service_url")]
    pub registry_service_url: String,

    #[serde(default)]
    pub health: HealthServerConfig,
}

fn registry_service_url() -> String {
    "http://registry:8080".into()
}

#[derive(Debug)]
pub struct WebData {
    pub authenticator: Option<Authenticator>,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

async fn command(
    sender: web::Data<DownstreamSender>,
    web::Query(opts): web::Query<CommandOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    registry: web::Data<registry::v1::Client>,
    token: BearerAuth,
) -> Result<HttpResponse, HttpEndpointError> {
    log::info!(
        "Send command '{}' to '{}' / '{}'",
        opts.command,
        opts.application,
        opts.device
    );

    let response = registry
        .get_device_and_gateways(
            &opts.application,
            &opts.device,
            Context {
                provided_token: Some(token.token().into()),
            },
        )
        .await;

    match response {
        Ok(Some(device_gateways)) => {
            let content_type = req
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            drogue_cloud_integration_common::commands::process_command(
                device_gateways.0,
                device_gateways.1,
                &sender,
                content_type,
                opts,
                body,
            )
            .await
        }
        Ok(None) => Ok(HttpResponse::NotAcceptable().finish()),
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

    let config = Config::from_env()?;
    let max_json_payload_size = config.max_json_payload_size;

    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let data = web::Data::new(WebData { authenticator });

    let registry = registry::v1::Client::new(
        Default::default(),
        Url::parse(&config.registry_service_url)?,
        None,
    );

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main server

    let main = HttpServer::new(move || {
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
            .service(index)
            .service(
                web::resource("/command")
                    .wrap(Condition::new(enable_auth, auth))
                    .route(web::post().to(command)),
            )
    })
    .bind(config.bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
