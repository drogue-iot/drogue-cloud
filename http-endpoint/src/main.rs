mod basic_auth;
mod error;
mod ttn;

use actix_web::{
    get, http::header, middleware, post, put, web, App, HttpResponse, HttpServer, Responder,
};

use drogue_cloud_endpoint_common::downstream::{
    DownstreamSender, Outcome, Publish, PublishResponse,
};
use serde::Deserialize;
use serde_json::json;

use actix_web_httpauth::middleware::HttpAuthentication;
use dotenv::dotenv;
use envconfig::Envconfig;
use futures::StreamExt;

use self::basic_auth::basic_validator;
use actix_web::middleware::Condition;

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

#[post("/publish/{device_id}/{channel}")]
async fn publish(
    endpoint: web::Data<DownstreamSender>,
    web::Path((device_id, channel)): web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    mut body: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Published to '{}'", channel);

    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let bytes = bytes.freeze();

    match endpoint
        .publish(
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
            bytes,
        )
        .await
    {
        // ok, and accepted
        Ok(PublishResponse {
            outcome: Outcome::Accepted,
        }) => Ok(HttpResponse::Accepted().finish()),

        // ok, but rejected
        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => Ok(HttpResponse::NotAcceptable().finish()),

        // internal error
        Err(err) => Ok(HttpResponse::InternalServerError()
            .content_type("text/plain")
            .body(err.to_string())),
    }
}

#[put("/telemetry/{tenant}/{device}")]
async fn telemetry(
    endpoint: web::Data<DownstreamSender>,
    web::Path((tenant, device)): web::Path<(String, String)>,
    req: web::HttpRequest,
    mut body: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!(
        "Sending telemetry for device '{}' belonging to tenant '{}'",
        device,
        tenant
    );

    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let bytes = bytes.freeze();

    match endpoint
        .publish(
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
            bytes,
        )
        .await
    {
        // ok, and accepted
        Ok(PublishResponse {
            outcome: Outcome::Accepted,
        }) => Ok(HttpResponse::Accepted().finish()),

        // ok, but rejected
        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => Ok(HttpResponse::NotAcceptable().finish()),

        // internal error
        Err(err) => Ok(HttpResponse::InternalServerError()
            .content_type("text/plain")
            .body(err.to_string())),
    }
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

    HttpServer::new(move || {
        //let jwt_auth = HttpAuthentication::bearer(jwt_validator);
        let basic_auth = HttpAuthentication::basic(basic_validator);

        App::new()
            .wrap(Condition::new(enable_auth, basic_auth))
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(sender.clone())
            .service(index)
            .service(publish)
            .service(telemetry)
            .service(ttn::publish)
    })
    .bind(config.bind_addr)?
    .run()
    .await?;

    Ok(())
}
