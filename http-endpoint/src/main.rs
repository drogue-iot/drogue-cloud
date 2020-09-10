mod http;

use crate::http::{HttpEndpoint, Outcome, Publish, PublishResponse};
use actix_web::dev::HttpResponseBuilder;
use actix_web::http::StatusCode;
use actix_web::{get, middleware, post, web, App, HttpResponse, HttpServer, Responder};
use log;

use futures_util::StreamExt;

#[get("/")]
async fn index() -> impl Responder {
    format!("Hello World!")
}

#[post("/publish/{channel}")]
async fn publish(
    endpoint: web::Data<HttpEndpoint>,
    web::Path(channel): web::Path<String>,
    mut body: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    log::info!("Published to '{}'", channel);

    match endpoint.publish(Publish { channel }, body).await {
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

const GLOBAL_MAX_JSON_PAYLOAD_SIZE: usize = 64 * 1024;

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let endpoint = HttpEndpoint::new()?;

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(GLOBAL_MAX_JSON_PAYLOAD_SIZE))
            .data(endpoint.clone())
            .service(index)
            .service(publish)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await?;

    Ok(())
}
