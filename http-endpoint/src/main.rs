mod ttn;

use actix_web::{get, middleware, post, put, web, App, HttpResponse, HttpServer, Responder};
use drogue_cloud_endpoint_common::downstream::{
    DownstreamSender, Outcome, Publish, PublishResponse,
};
use futures::StreamExt;
use serde_json::json;

const GLOBAL_MAX_JSON_PAYLOAD_SIZE: usize = 64 * 1024;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[post("/publish/{channel}")]
async fn publish(
    endpoint: web::Data<DownstreamSender>,
    web::Path(channel): web::Path<String>,
    mut body: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Published to '{}'", channel);

    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let bytes = bytes.freeze();

    match endpoint.publish(Publish { channel }, bytes).await {
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
        "Sending telemetry for an unauthenticated device '{}' belonging to tenant '{}'",
        device,
        tenant
    );

    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let bytes = bytes.freeze();

    match endpoint.publish(Publish { channel: tenant }, bytes).await {
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

    log::info!("Starting HTTP service endpoint");

    let sender = DownstreamSender::new()?;

    let addr = std::env::var("BIND_ADDR").ok();
    let addr = addr.as_deref();

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(GLOBAL_MAX_JSON_PAYLOAD_SIZE))
            .data(sender.clone())
            .service(index)
            .service(publish)
            .service(telemetry)
            .service(ttn::publish)
    })
    .bind(addr.unwrap_or("127.0.0.1:8080"))?
    .run()
    .await?;

    Ok(())
}
