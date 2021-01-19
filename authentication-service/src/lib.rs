mod db;
pub mod endpoints;
pub mod service;

use envconfig::Envconfig;

#[derive(Clone)]
pub struct WebData<S>
where
    S: service::AuthenticationService,
{
    pub service: S,
}

#[derive(Clone, Envconfig)]
pub struct Config {
    #[envconfig(from = "BIND_ADDR", default = "127.0.0.1:8080")]
    pub bind_addr: String,
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "65536")]
    pub max_json_payload_size: usize,
    #[envconfig(from = "HEALTH_BIND_ADDR", default = "127.0.0.1:9090")]
    pub health_bind_addr: String,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $max_json_payload_size:expr) => {
        App::new()
            .data(web::JsonConfig::default().limit($max_json_payload_size))
            .service(web::scope("/api/v1").service(endpoints::authenticate))
            //fixme : bind to a different port
            .service(endpoints::health)
            .data($data.clone())
    };
}
