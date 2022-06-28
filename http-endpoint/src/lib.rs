mod command;
mod downstream;
mod telemetry;
mod ttn;
mod x509;

use actix_web::{middleware, web, App, HttpResponse, HttpServer, Responder};
use drogue_cloud_endpoint_common::{
    auth::{AuthConfig, DeviceAuthenticator},
    command::{Commands, KafkaCommandSource, KafkaCommandSourceConfig},
    sender::{DownstreamSender, ExternalClientPoolConfig},
    sink::KafkaSink,
};
use drogue_cloud_service_api::{
    health::BoxedHealthChecked,
    kafka::KafkaClientConfig,
    webapp::{self as actix_web, opentelemetry::RequestTracing, prom::PrometheusMetricsBuilder},
};
use drogue_cloud_service_common::{
    actix, app::run_main, defaults, health::HealthServerConfig, tls::TlsMode, tls::WithTlsMode,
};
use futures_util::{FutureExt, TryFutureExt};
use serde::Deserialize;
use serde_json::json;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,
    #[serde(default = "defaults::max_payload_size")]
    pub max_payload_size: usize,
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default)]
    pub disable_tls: bool,
    #[serde(default)]
    pub cert_bundle_file: Option<String>,
    #[serde(default)]
    pub key_file: Option<String>,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub auth: AuthConfig,

    pub command_source_kafka: KafkaCommandSourceConfig,

    pub kafka_downstream_config: KafkaClientConfig,
    pub kafka_command_config: KafkaClientConfig,

    pub instance: String,

    #[serde(default = "defaults::check_kafka_topic_ready")]
    pub check_kafka_topic_ready: bool,

    #[serde(default)]
    pub workers: Option<usize>,

    #[serde(default)]
    pub endpoint_pool: ExternalClientPoolConfig,
}

async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    log::info!("Starting HTTP service endpoint");

    let sender = DownstreamSender::new(
        KafkaSink::from_config(
            config.kafka_downstream_config,
            config.check_kafka_topic_ready,
        )?,
        config.instance,
        config.endpoint_pool,
    )?;
    let commands = Commands::new();

    let max_payload_size = config.max_payload_size;
    let max_json_payload_size = config.max_json_payload_size;
    let http_server_commands = commands.clone();

    let device_authenticator = DeviceAuthenticator::new(config.auth).await?;

    let prometheus = PrometheusMetricsBuilder::new("http_endpoint")
        .registry(prometheus::default_registry().clone())
        .build()
        .unwrap();

    let http_server = HttpServer::new(move || {
        let app = App::new()
            .wrap(RequestTracing::new())
            .wrap(prometheus.clone())
            .wrap(middleware::Logger::default())
            .app_data(web::PayloadConfig::new(max_payload_size))
            .app_data(web::JsonConfig::default().limit(max_json_payload_size))
            .app_data(web::Data::new(sender.clone()))
            .app_data(web::Data::new(http_server_commands.clone()));

        let app = app.app_data(web::Data::new(device_authenticator.clone()));

        app.service(web::resource("/").route(web::get().to(index)))
            // the standard endpoint
            .service(
                web::scope("/v1")
                    .service(
                        web::resource("/{channel}").route(web::post().to(telemetry::publish_plain)),
                    )
                    .service(
                        web::resource("/{channel}/{suffix:.*}")
                            .route(web::post().to(telemetry::publish_tail)),
                    ),
            )
            // The Things Network variant
            .service(
                web::scope("/ttn")
                    .route("/", web::post().to(ttn::publish_v2))
                    .route("/v2", web::post().to(ttn::publish_v2))
                    .route("/v3", web::post().to(ttn::publish_v3)),
            )
    })
    .on_connect(|con, ext| {
        if let Some(cert) = x509::from_socket(con) {
            if !cert.0.is_empty() {
                log::debug!("Added {} client certificates", cert.0.len());
                ext.insert(cert);
            }
        }
    });

    let mut http_server = actix::bind_http(
        http_server,
        config.bind_addr,
        config.disable_tls.with_tls_mode(TlsMode::Client),
        config.key_file,
        config.cert_bundle_file,
    )?;

    if let Some(workers) = config.workers {
        http_server = http_server.workers(workers)
    }

    let command_source = KafkaCommandSource::new(
        commands,
        config.kafka_command_config,
        config.command_source_kafka,
    )?;

    // run

    let main = http_server.run().err_into().boxed_local();
    run_main([main], config.health, [command_source.boxed()]).await?;

    // done

    Ok(())
}
