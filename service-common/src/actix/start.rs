use crate::{
    actix::{bind_http, HttpConfig},
    tls::{TlsMode, WithTlsMode},
};
use actix_cors::Cors;
use actix_web_extras::middleware::Condition;
use drogue_cloud_service_api::webapp::{
    dev::Extensions,
    middleware,
    prom::PrometheusMetricsBuilder,
    web::{self, ServiceConfig},
    App, HttpServer,
};
use futures::{future::LocalBoxFuture, FutureExt, TryFutureExt};
use std::{any::Any, sync::Arc};

#[derive(Clone)]
pub enum CorsBuilder {
    Disabled,
    Permissive,
    Custom(Arc<dyn Fn() -> Cors + Send + Sync>),
}

impl Default for CorsBuilder {
    fn default() -> Self {
        Self::Disabled
    }
}

impl<F> From<F> for CorsBuilder
where
    F: Fn() -> Cors + Send + Sync + 'static,
{
    fn from(f: F) -> Self {
        CorsBuilder::Custom(Arc::new(f))
    }
}

pub struct HttpBuilder<F>
where
    F: Fn(&mut ServiceConfig) + Send + Clone + 'static,
{
    config: HttpConfig,
    app_builder: Box<F>,
    cors_builder: CorsBuilder,
    on_connect: Option<Box<dyn Fn(&dyn Any, &mut Extensions) + Send + Sync + 'static>>,
    tls_mode: TlsMode,
}

impl<F> HttpBuilder<F>
where
    F: Fn(&mut ServiceConfig) + Send + Clone + 'static,
{
    pub fn new(config: HttpConfig, app_builder: F) -> Self {
        Self {
            config,
            app_builder: Box::new(app_builder),
            cors_builder: Default::default(),
            on_connect: None,
            tls_mode: TlsMode::NoClient,
        }
    }

    pub fn cors<I: Into<CorsBuilder>>(mut self, cors_builder: I) -> Self {
        self.cors_builder = cors_builder.into();
        self
    }

    pub fn on_connect<O>(mut self, on_connect: O) -> Self
    where
        O: Fn(&dyn Any, &mut Extensions) + Send + Sync + 'static,
    {
        self.on_connect = Some(Box::new(on_connect));
        self
    }

    pub fn tls_mode<I: Into<TlsMode>>(mut self, tls_mode: I) -> Self {
        self.tls_mode = tls_mode.into();
        self
    }

    pub fn run(self) -> Result<LocalBoxFuture<'static, Result<(), anyhow::Error>>, anyhow::Error> {
        let max_payload_size = self.config.max_payload_size;
        let max_json_payload_size = self.config.max_json_payload_size;

        let prometheus = PrometheusMetricsBuilder::new(
            self.config
                .metrics_namespace
                .as_deref()
                .unwrap_or("drogue_cloud_http"),
        )
        .registry(prometheus::default_registry().clone())
        .build()
        // FIXME: replace with direct conversion once nlopes/actix-web-prom#67 is merged
        .map_err(|err| anyhow::anyhow!("Failed to build prometheus middleware: {err}"))?;

        let mut main = HttpServer::new(move || {
            let cors = match self.cors_builder.clone() {
                CorsBuilder::Disabled => None,
                CorsBuilder::Permissive => Some(Cors::permissive()),
                CorsBuilder::Custom(f) => Some(f()),
            };

            let app = App::new()
                .wrap(drogue_cloud_service_api::webapp::opentelemetry::RequestTracing::new())
                .wrap(prometheus.clone())
                .wrap(Condition::from_option(cors))
                .wrap(middleware::Logger::default())
                .app_data(web::PayloadConfig::new(max_payload_size))
                .app_data(web::JsonConfig::default().limit(max_json_payload_size));

            app.configure(|cfg| (self.app_builder)(cfg))
        });

        if let Some(on_connect) = self.on_connect {
            main = main.on_connect(on_connect);
        }

        let mut main = bind_http(
            main,
            self.config.bind_addr,
            self.config.disable_tls.with_tls_mode(self.tls_mode),
            self.config.key_file,
            self.config.cert_bundle_file,
        )?;

        if let Some(workers) = self.config.workers {
            main = main.workers(workers)
        }

        Ok(main.run().err_into().boxed_local())
    }
}
