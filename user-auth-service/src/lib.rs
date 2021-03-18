pub mod endpoints;
pub mod service;

use crate::service::AuthorizationServiceConfig;
use drogue_cloud_service_common::{defaults, openid::Authenticator};
use serde::Deserialize;

pub struct WebData<S>
where
    S: service::AuthorizationService,
{
    pub service: S,
    pub authenticator: Option<Authenticator>,
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub service: AuthorizationServiceConfig,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::health_bind_addr")]
    pub health_bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $max_json_payload_size:expr, $enable_auth: expr, $auth: expr) => {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .data(web::JsonConfig::default().limit($max_json_payload_size))
            .app_data($data.clone())
            .service(
                web::scope("/api/v1/user")
                    .wrap(actix_web::middleware::Condition::new($enable_auth, $auth))
                    .service(endpoints::authorize),
            )
            //fixme : bind to a different port
            .service(endpoints::health)
    };
}
