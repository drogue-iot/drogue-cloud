use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};
use console_common::{Endpoints, HttpEndpoint, MqttEndpoint};

#[get("/info")]
pub async fn get_info() -> impl Responder {
    HttpResponse::Ok().json(Endpoints {
        http: Some(HttpEndpoint {
            url: "https://http.foo.bar".into(),
        }),
        mqtt: Some(MqttEndpoint {
            host: "mqtt.foo.bar".into(),
            port: 443,
        }),
    })
}
