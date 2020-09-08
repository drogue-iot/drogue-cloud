use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};
use log;

pub struct PublishResponse {}

#[get("/publish/{channel}")]
async fn publish(web::Path(channel): web::Path<String>) -> impl Responder {
    log::info!("Published to '{}'", channel);
    HttpResponse::Ok()
}

const GLOBAL_MAX_JSON_PAYLOAD_SIZE: usize = 64 * 1024;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .data(web::JsonConfig::default().limit(GLOBAL_MAX_JSON_PAYLOAD_SIZE))
            .service(publish)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
