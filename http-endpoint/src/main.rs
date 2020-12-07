mod basic_auth;
mod ttn;

use std::convert::TryInto;

use actix_web::middleware::Condition;
use actix_web::{
    get, http::header, middleware, post, put, web, App, HttpResponse, HttpServer, Responder,
};
use actix_web_httpauth::middleware::HttpAuthentication;

use drogue_cloud_endpoint_common::downstream::{DownstreamSender, Publish};
use drogue_cloud_endpoint_common::error::HttpEndpointError;
use serde::Deserialize;
use serde_json::json;

use dotenv::dotenv;
use envconfig::Envconfig;

use crate::basic_auth::{basic_validator, DeviceAuthenticator};

#[derive(Envconfig, Clone, Debug)]
struct Config {
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "ENABLE_AUTH", default = "false")]
    pub enable_auth: bool,
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

#[derive(Deserialize)]
pub struct PublishOptions {
    model_id: Option<String>,
}

#[post("/publish/{device_id}/{channel}")]
async fn publish(
    endpoint: web::Data<DownstreamSender>,
    web::Path((device_id, channel)): web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    log::info!("Published to '{}'", channel);

    endpoint
        .publish_http(
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
        )
        .await
}

#[put("/telemetry/{tenant}/{device}")]
async fn telemetry(
    endpoint: web::Data<DownstreamSender>,
    web::Path((tenant, device)): web::Path<(String, String)>,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    log::info!(
        "Sending telemetry for device '{}' belonging to tenant '{}'",
        device,
        tenant
    );
    endpoint
        .publish_http(
            Publish {
                channel: tenant,
                device_id: device,
                model_id: None,
                content_type: req
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string()),
            },
            body,
        )
        .await
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting HTTP service endpoint");

    let sender = DownstreamSender::new()?;

    let config = Config::init_from_env()?;
    let enable_auth = config.enable_auth;
    let max_json_payload_size = config.max_json_payload_size;

    // create authenticator, fails if authentication is enabled, but configuration is missing
    let authenticator = match enable_auth {
        true => {
            let authenticator: DeviceAuthenticator =
                basic_auth::AuthConfig::init_from_env()?.try_into()?;
            Some(authenticator)
        }
        false => None,
    };

    HttpServer::new(move || {
        //let jwt_auth = HttpAuthentication::bearer(jwt_validator);
        let basic_auth = HttpAuthentication::basic(basic_validator);

        let app = App::new()
            .wrap(Condition::new(enable_auth, basic_auth))
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(sender.clone());
        // add authenticator, if we have one
        let app = if let Some(authenticator) = &authenticator {
            app.app_data(authenticator.clone())
        } else {
            app
        };
        app.service(index)
            .service(publish)
            .service(telemetry)
            .service(ttn::publish)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
