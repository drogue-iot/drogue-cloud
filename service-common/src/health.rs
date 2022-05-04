use crate::defaults;
use actix_web::HttpServer;
use drogue_cloud_service_api::{
    health::{HealthCheckError, HealthChecked},
    webapp::{self as actix_web, prom::PrometheusMetricsBuilder},
};
use futures::StreamExt;
use prometheus::Registry;
use serde::Deserialize;
use serde_json::{json, Value};
use std::future::Future;
use std::sync::Arc;

use prometheus::{Encoder, TextEncoder};
use tracing::instrument;

#[derive(Clone, Debug, Deserialize)]
pub struct HealthServerConfig {
    #[serde(default = "defaults::health_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::health_workers")]
    pub workers: usize,
}

impl Default for HealthServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: defaults::health_bind_addr(),
            workers: defaults::health_workers(),
        }
    }
}

/// A server, running health check endpoints.
pub struct HealthServer {
    config: HealthServerConfig,
    checker: HealthChecker,
    registry: Option<Registry>,
}

/// Internal handling of health checking.
pub struct HealthChecker {
    checks: Vec<Box<dyn HealthChecked>>,
}

impl HealthChecker {
    #[instrument(level = "trace", skip(self), ret)]
    pub async fn is_ready(&self) -> Vec<Result<(), HealthCheckError>> {
        futures::stream::iter(self.checks.iter())
            .then(|check| check.is_ready())
            .collect()
            .await
    }

    #[instrument(level = "trace", skip(self), ret)]
    pub async fn is_alive(&self) -> Vec<Result<(), HealthCheckError>> {
        futures::stream::iter(self.checks.iter())
            .then(|check| check.is_alive())
            .collect()
            .await
    }
}

async fn run_checks<F, Fut>(checker: Arc<HealthChecker>, f: F) -> (http::StatusCode, Value)
where
    F: FnOnce(Arc<HealthChecker>) -> Fut,
    Fut: Future<Output = Vec<Result<(), HealthCheckError>>>,
{
    let result: Result<Vec<()>, _> = f(checker).await.into_iter().collect();

    match result {
        Ok(_) => (http::StatusCode::OK, json!({ "success": true})),
        Err(_) => (
            http::StatusCode::SERVICE_UNAVAILABLE,
            json!({"success": false}),
        ),
    }
}

macro_rules! health_endpoint {
    ($sys:ident) => {
        async fn index() -> $sys::HttpResponse {
            $sys::HttpResponse::Ok().json(&json!({}))
        }

        async fn readiness(checker: Data<HealthChecker>) -> $sys::HttpResponse {
            let (code, body) = run_checks(checker.into_inner(), |checker| async move {
                checker.is_ready().await
            })
            .await;
            $sys::HttpResponse::build(code.into()).json(&body)
        }

        async fn liveness(checker: Data<HealthChecker>) -> $sys::HttpResponse {
            let (code, body) = run_checks(checker.into_inner(), |checker| async move {
                checker.is_alive().await
            })
            .await;
            $sys::HttpResponse::build(code.into()).json(&body)
        }
    };
}

macro_rules! health_app {
    ($checker:expr, $app_data:ident) => {
        App::new()
            .$app_data($checker.clone())
            .route("/", web::get().to(index))
            .route("/readiness", web::get().to(readiness))
            .route("/liveness", web::get().to(liveness))
    };
}

impl HealthServer {
    pub fn new(
        config: HealthServerConfig,
        checks: Vec<Box<dyn HealthChecked>>,
        registry: Option<Registry>,
    ) -> Self {
        Self {
            config,
            checker: HealthChecker { checks },
            registry,
        }
    }

    /// Run the health server. This must be called from inside actix.
    ///
    /// For running on a bare tokio setup, use [`run_with_tokio`].
    pub async fn run(self) -> anyhow::Result<()> {
        use actix_web::web;
        use actix_web::web::Data;
        health_endpoint!(actix_web);

        let checker = Data::new(self.checker);

        let prometheus = match self.registry {
            Some(metrics) => PrometheusMetricsBuilder::new("health")
                .registry(metrics)
                .endpoint("/metrics")
                .build()
                .unwrap(),
            _ => PrometheusMetricsBuilder::new("noop").build().unwrap(),
        };

        HttpServer::new(move || {
            use actix_web::App;

            health_app!(checker, app_data).wrap(prometheus.clone())
        })
        .bind(self.config.bind_addr)?
        .workers(self.config.workers)
        .run()
        .await?;

        Ok(())
    }

    /// Run the health server. This must be called from inside ntex.
    pub async fn run_ntex(self) -> anyhow::Result<()> {
        use ntex::web as ntex_web;
        use ntex::web;
        use ntex::web::types::State as Data;
        health_endpoint!(ntex_web);

        let checker = ntex::web::types::State::new(self.checker);
        ntex::web::server(move || {
            use ntex::web::App;
            health_app!(checker, app_state).route("/metrics", web::get().to(HealthServer::metrics))
        })
        .bind(self.config.bind_addr)?
        .workers(self.config.workers)
        .run()
        .await?;

        Ok(())
    }

    async fn metrics() -> ntex::web::HttpResponse {
        let encoder = TextEncoder::new();
        let mut buffer = vec![];
        encoder
            .encode(&prometheus::gather(), &mut buffer)
            .expect("Failed to encode metrics");

        let response =
            String::from_utf8(buffer.clone()).expect("Failed to convert bytes to string");
        buffer.clear();

        ntex::web::HttpResponse::Ok()
            .set_header(ntex::http::header::CONTENT_TYPE, "text/plain")
            .body(response)
    }
}
