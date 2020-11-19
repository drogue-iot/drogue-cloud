mod basic_auth;
mod error;
mod ttn;

use actix_web::{get, middleware, post, put, web, App, HttpResponse, HttpServer, Responder};

use drogue_cloud_endpoint_common::downstream::{
    DownstreamSender, Outcome, Publish, PublishResponse,
};
use serde::Deserialize;
use serde_json::json;

use actix_web_httpauth::middleware::HttpAuthentication;
use dotenv::dotenv;

use futures::StreamExt;
use log;

use self::basic_auth::basic_validator;
use actix_web::middleware::Condition;

const GLOBAL_MAX_JSON_PAYLOAD_SIZE: usize = 64 * 1024;

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

//TODO : use envconfig
#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting HTTP service endpoint");

    let sender = DownstreamSender::new()?;

    let addr = std::env::var("BIND_ADDR").ok();
    let addr = addr.as_deref().unwrap_or("127.0.0.1:8080");

    let enable_auth = match std::env::var_os("ENABLE_AUTH") {
        Some(str) => str == "true",
        None => false,
    };

    HttpServer::new(move || {
        //let jwt_auth = HttpAuthentication::bearer(jwt_validator);
        let basic_auth = HttpAuthentication::basic(basic_validator);

        App::new()
            .wrap(Condition::new(enable_auth, basic_auth))
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(GLOBAL_MAX_JSON_PAYLOAD_SIZE))
            .data(sender.clone())
            .service(index)
            .service(publish)
            .service(telemetry)
            .service(ttn::publish)
    })
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}
