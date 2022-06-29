mod command;
mod downstream;
mod telemetry;
mod ttn;
mod x509;

use actix_web::{web, HttpResponse, Responder};
use drogue_cloud_endpoint_common::{
    auth::{AuthConfig, DeviceAuthenticator},
    command::{Commands, KafkaCommandSource, KafkaCommandSourceConfig},
    sender::{DownstreamSender, ExternalClientPoolConfig},
    sink::KafkaSink,
};
use drogue_cloud_service_api::{
    health::BoxedHealthChecked,
    kafka::KafkaClientConfig,
    webapp::{self as actix_web},
};
use drogue_cloud_service_common::{
    actix::{HttpBuilder, HttpConfig},
    app::run_main,
    defaults,
    health::HealthServerConfig,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
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
    pub endpoint_pool: ExternalClientPoolConfig,

    #[serde(default)]
    pub http: HttpConfig,
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

    let http_server_commands = commands.clone();

    let device_authenticator = DeviceAuthenticator::new(config.auth).await?;

    let main = HttpBuilder::new(config.http, move |cfg| {
        cfg.app_data(web::Data::new(sender.clone()))
            .app_data(web::Data::new(http_server_commands.clone()))
            .app_data(web::Data::new(device_authenticator.clone()))
            .service(web::resource("/").route(web::get().to(index)))
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
            );
    })
    .on_connect(|con, ext| {
        if let Some(cert) = x509::from_socket(con) {
            if !cert.0.is_empty() {
                log::debug!("Added {} client certificates", cert.0.len());
                ext.insert(cert);
            }
        }
    })
    .run()?;

    // command source

    let command_source = KafkaCommandSource::new(
        commands,
        config.kafka_command_config,
        config.command_source_kafka,
    )?;

    // run

    run_main([main], config.health, [command_source.boxed()]).await?;

    // done

    Ok(())
}
