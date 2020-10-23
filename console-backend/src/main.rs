mod endpoints;
mod info;
mod kube;

use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};

use serde_json::json;

use crate::endpoints::{EndpointSourceType, OpenshiftEndpointSource};
use actix_cors::Cors;
use actix_web::web::Data;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let addr = std::env::var("BIND_ADDR").ok();
    let addr = addr.as_ref().map(|s| s.as_str());

    // the endpoint source we choose
    let endpoint_source: Data<EndpointSourceType> =
        Data::new(Box::new(OpenshiftEndpointSource::new()?));

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(Cors::new().send_wildcard().finish())
            .data(web::JsonConfig::default().limit(4096))
            .app_data(endpoint_source.clone())
            .service(index)
            .service(health)
            .service(info::get_info)
    })
    .bind(addr.unwrap_or("127.0.0.1:8080"))?
    .run()
    .await?;

    Ok(())
}
