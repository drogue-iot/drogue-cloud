pub mod endpoints;
pub mod service;

use crate::service::PostgresAuthenticationService;
use actix_web::{web, App, HttpServer};
use drogue_cloud_service_api::{
    health::HealthChecked,
    webapp::{self as actix_web, prom::PrometheusMetricsBuilder},
};
use drogue_cloud_service_common::{
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::{Authenticator, AuthenticatorConfig},
    openid_auth,
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

    pub oauth: AuthenticatorConfig,

    #[serde(flatten)]
    pub auth_service_config: AuthenticationServiceConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub workers: Option<usize>,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $max_json_payload_size:expr, $enable_auth: expr, $auth: expr, $prometheus: expr) => {{
        use drogue_cloud_service_common::middleware::Optional;

        let prom: Optional<drogue_cloud_service_api::webapp::prom::PrometheusMetrics> =
            Optional::new($prometheus);

        App::new()
            .wrap(drogue_cloud_service_api::webapp::opentelemetry::RequestTracing::new())
            .wrap(prom)
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit($max_json_payload_size))
            .app_data($data.clone())
            .service(
                web::scope("/api/v1")
                    .wrap(actix_web::middleware::Condition::new($enable_auth, $auth))
                    .service(endpoints::authenticate)
                    .service(endpoints::authorize_as),
            )
    }};
}

/// Build the health checks used for this service.
pub fn health_checks(service: PostgresAuthenticationService) -> Vec<Box<dyn HealthChecked>> {
    vec![Box::new(service)]
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let max_json_payload_size = config.max_json_payload_size;

    let authenticator = config.oauth.into_client().await?;
    let enable_auth = authenticator.is_some();

    let data = web::Data::new(WebData {
        authenticator,
        service: service::PostgresAuthenticationService::new(config.auth_service_config)?,
    });

    let data_service = data.service.clone();

    let prometheus = PrometheusMetricsBuilder::new("authentication_service")
        .registry(prometheus::default_registry().clone())
        .build()
        .unwrap();

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthenticationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        app!(
            data,
            max_json_payload_size,
            enable_auth,
            auth,
            Some(prometheus.clone())
        )
    })
    .bind(config.bind_addr)?;

    let main = if let Some(workers) = config.workers {
        main.workers(workers).run()
    } else {
        main.run()
    };

    // run

    if let Some(health) = config.health {
        let health = HealthServer::new(
            health,
            vec![Box::new(data_service)],
            Some(prometheus::default_registry().clone()),
        );
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    // exiting

    Ok(())
}
