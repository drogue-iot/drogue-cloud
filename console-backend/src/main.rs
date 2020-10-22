mod info;

use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};

use serde_json::json;

use actix_cors::Cors;

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

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(Cors::new().send_wildcard().finish())
            .data(web::JsonConfig::default().limit(4096))
            .service(index)
            .service(health)
            .service(info::get_info)
    })
    .bind(addr.unwrap_or("127.0.0.1:8080"))?
    .run()
    .await?;

    Ok(())
}
