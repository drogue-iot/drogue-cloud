pub mod endpoints;
pub mod service;

use crate::service::PostgresAuthenticationService;
use drogue_cloud_service_api::health::HealthChecked;
use drogue_cloud_service_common::{defaults, health::HealthServerConfig, openid::Authenticator};
use serde::Deserialize;

pub struct WebData<S>
where
    S: service::AuthenticationService,
{
    pub service: S,
    pub authenticator: Option<Authenticator>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,

    #[serde(default)]
    pub health: HealthServerConfig,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $max_json_payload_size:expr, $enable_auth: expr, $auth: expr) => {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::Data::new(
                web::JsonConfig::default().limit($max_json_payload_size),
            ))
            .app_data($data.clone())
            .service(
                web::scope("/api/v1")
                    .wrap(actix_web::middleware::Condition::new($enable_auth, $auth))
                    .service(endpoints::authenticate),
            )
    };
}

/// Build the health checks used for this service.
pub fn health_checks(service: PostgresAuthenticationService) -> Vec<Box<dyn HealthChecked>> {
    vec![Box::new(service)]
}
