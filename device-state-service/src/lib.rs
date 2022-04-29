pub mod endpoints;
pub mod service;

use crate::service::{DeviceStateService, PostgresServiceConfiguration};
use actix_web::{web, App, HttpServer};
use anyhow::Context;
use drogue_cloud_service_api::{
    health::HealthChecked,
    webapp::{self as actix_web, prom::PrometheusMetricsBuilder},
};
use drogue_cloud_service_common::{
    actix_auth::authentication::AuthN,
    app::run_main,
    client::{UserAuthClient, UserAuthClientConfig},
    defaults,
    health::HealthServerConfig,
    openid::AuthenticatorConfig,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    pub oauth: AuthenticatorConfig,

    #[serde(flatten)]
    pub service_config: PostgresServiceConfiguration,

    #[serde(default)]
    pub workers: Option<usize>,

    #[serde(default)]
    pub max_json_payload_size: usize,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $max_json_payload_size:expr, $auth: expr, $prometheus: expr) => {{
        use drogue_cloud_service_api::webapp::{extras::middleware::Condition, middleware};
        use $crate::endpoints;

        let prom: Condition<drogue_cloud_service_api::webapp::prom::PrometheusMetrics> =
            Condition::from_option($prometheus);

        App::new()
            .wrap(drogue_cloud_service_api::webapp::opentelemetry::RequestTracing::new())
            .wrap(prom)
            .wrap(middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit($max_json_payload_size))
            .app_data($data.clone())
            .service(
                web::scope("/api/state/v1alpha1")
                    .wrap($auth)
                    .service(web::resource("/states").route(web::put().to(endpoints::init)))
                    .service(
                        web::resource("/states/{session}").route(web::post().to(endpoints::ping)),
                    )
                    .service(
                        web::resource("/states/{session}/{id}")
                            .route(web::put().to(endpoints::create))
                            .route(web::delete().to(endpoints::delete)),
                    ),
            )
    }};
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::info!("Running device state service!");

    let enable_access_token = config.enable_access_token;

    // set up authentication

    let authenticator = config.oauth.into_client().await?;
    let user_auth = if let Some(user_auth) = config.user_auth {
        let user_auth = UserAuthClient::from_config(user_auth).await?;
        Some(user_auth)
    } else {
        None
    };

    let mut checks = Vec::<Box<dyn HealthChecked>>::new();

    let service = service::PostgresDeviceStateService::new(config.service_config)?;
    checks.push(Box::new(service.clone()));

    let service: Arc<dyn DeviceStateService> = Arc::new(service);
    let service: web::Data<dyn DeviceStateService> = web::Data::from(service);

    // monitoring

    let prometheus = PrometheusMetricsBuilder::new("device_state_service")
        .registry(prometheus::default_registry().clone())
        .build()
        .unwrap();

    // main server

    let max_json_payload_size = config.max_json_payload_size;
    let main = HttpServer::new(move || {
        let auth = AuthN {
            openid: authenticator.as_ref().cloned(),
            token: user_auth.clone(),
            enable_access_token,
        };
        let app = app!(
            service,
            max_json_payload_size,
            auth,
            Some(prometheus.clone())
        );
        app.app_data(service.clone())
    })
    .bind(config.bind_addr)
    .context("error starting server")?;

    let main = if let Some(workers) = config.workers {
        main.workers(workers).run()
    } else {
        main.run()
    };

    // run

    run_main(main, config.health, checks).await?;

    // exiting

    Ok(())
}
