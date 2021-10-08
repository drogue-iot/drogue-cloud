pub mod endpoints;
pub mod service;

use crate::service::PostgresAuthenticationService;
use actix_web::{web, App, HttpServer};
use drogue_cloud_service_api::health::HealthChecked;
use drogue_cloud_service_common::{config::ConfigFromEnv, openid_auth};
use drogue_cloud_service_common::{
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::Authenticator,
};
use futures::TryFutureExt;
use serde::Deserialize;
use service::AuthenticationServiceConfig;

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
    pub health: Option<HealthServerConfig>,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $max_json_payload_size:expr, $enable_auth: expr, $auth: expr) => {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit($max_json_payload_size))
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

pub async fn run(config: Config) -> anyhow::Result<()> {
    let max_json_payload_size = config.max_json_payload_size;
    let enable_auth = config.enable_auth;

    let authenticator = if enable_auth {
        Some(Authenticator::new().await?)
    } else {
        None
    };

    let data = web::Data::new(WebData {
        authenticator,
        service: service::PostgresAuthenticationService::new(
            AuthenticationServiceConfig::from_env()?,
        )?,
    });

    let data_service = data.service.clone();

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthenticationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        app!(data, max_json_payload_size, enable_auth, auth)
    })
    .bind(config.bind_addr)?
    .run();

    // run

    if let Some(health) = config.health {
        let health = HealthServer::new(health, vec![Box::new(data_service)]);
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    // exiting

    Ok(())
}
