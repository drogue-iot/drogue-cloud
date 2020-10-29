mod endpoints;
mod info;
mod kube;
mod spy;

use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};

use serde_json::json;

use crate::endpoints::{
    EndpointSourceType, EnvEndpointSource, KubernetesEndpointSource, OpenshiftEndpointSource,
};
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
    let addr = addr.as_deref();

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;
    log::info!("Using endpoint source: {:?}", endpoint_source);
    let endpoint_source: Data<EndpointSourceType> = Data::new(endpoint_source);

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(Cors::new().send_wildcard().finish())
            .data(web::JsonConfig::default().limit(4096))
            .app_data(endpoint_source.clone())
            .service(index)
            .service(health)
            .service(info::get_info)
            .service(spy::stream_events)
    })
    .bind(addr.unwrap_or("127.0.0.1:8080"))?
    .run()
    .await?;

    Ok(())
}

fn create_endpoint_source() -> anyhow::Result<EndpointSourceType> {
    match std::env::var_os("ENDPOINT_SOURCE") {
        Some(name) if name == "openshift" => Ok(Box::new(OpenshiftEndpointSource::new()?)),
        Some(name) if name == "kubernetes" => Ok(Box::new(KubernetesEndpointSource::new()?)),
        Some(name) => Err(anyhow::anyhow!(
            "Unsupported endpoint source: '{}'",
            name.to_str().unwrap_or_default()
        )),
        None => Ok(Box::new(EnvEndpointSource)),
    }
}
