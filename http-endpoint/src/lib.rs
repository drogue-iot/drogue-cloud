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
    kafka::KafkaClientConfig,
    webapp::{self as actix_web, opentelemetry::RequestTracing, prom::PrometheusMetricsBuilder},
};
use drogue_cloud_service_common::{app::run_main, defaults, health::HealthServerConfig};
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

    let http_server = match (config.disable_tls, config.key_file, config.cert_bundle_file) {
        (false, Some(key), Some(cert)) => {
            if cfg!(feature = "openssl") {
                use open_ssl::ssl;
                let method = ssl::SslMethod::tls_server();
                let mut builder = ssl::SslAcceptor::mozilla_intermediate_v5(method)?;
                builder.set_private_key_file(key, ssl::SslFiletype::PEM)?;
                builder.set_certificate_chain_file(cert)?;
                // we ask for client certificates, but don't enforce them
                builder.set_verify_callback(ssl::SslVerifyMode::PEER, |_, ctx| {
                    log::debug!(
                        "Accepting client certificates: {:?}",
                        ctx.current_cert()
                            .map(|cert| format!("{:?}", cert.subject_name()))
                            .unwrap_or_else(|| "<unknown>".into())
                    );
                    true
                });

                http_server.bind_openssl(config.bind_addr, builder)?
            } else {
                panic!("TLS is required, but no TLS implementation enabled")
            }
        }
        (true, None, None) => http_server.bind(config.bind_addr)?,
        (false, _, _) => panic!("Wrong TLS configuration: TLS enabled, but key or cert is missing"),
        (true, Some(_), _) | (true, _, Some(_)) => {
            // the TLS configuration must be consistent, to prevent configuration errors.
            panic!("Wrong TLS configuration: key or cert specified, but TLS is disabled")
        }
    };

    let http_server = if let Some(workers) = config.workers {
        http_server.workers(workers).run()
    } else {
        http_server.run()
    };

    let command_source = KafkaCommandSource::new(
        commands,
        config.kafka_command_config,
        config.command_source_kafka,
    )?;

    // run

    run_main(http_server, config.health, vec![Box::new(command_source)]).await?;

    // done

    Ok(())
}
