mod command;
mod downstream;
mod telemetry;
mod ttn;
mod x509;

#[cfg(feature = "rustls")]
use actix_tls::rustls::Session;
use actix_web::{
    get, middleware,
    web::{self, Data},
    App, HttpResponse, HttpServer, Responder,
};
use dotenv::dotenv;
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    command_endpoint::{CommandServer, CommandServerConfig},
    commands::Commands,
    downstream::{DownstreamSender, KafkaSink},
};
use drogue_cloud_service_common::{
    config::ConfigFromEnv,
    defaults,
    health::{HealthServer, HealthServerConfig},
};
use futures::TryFutureExt;
use serde::Deserialize;
use serde_json::json;
use std::ops::DerefMut;

drogue_cloud_endpoint_common::retriever!();

#[cfg(feature = "rustls")]
drogue_cloud_endpoint_common::retriever_rustls!(actix_tls::connect::ssl::rustls::TlsStream<T>);

#[cfg(feature = "openssl")]
drogue_cloud_endpoint_common::retriever_openssl!(actix_tls::connect::ssl::openssl::SslStream<T>);

#[cfg(feature = "ntex")]
retriever_none!(ntex::rt::net::TcpStream);

#[derive(Clone, Debug, Deserialize)]
struct Config {
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
    pub health: HealthServerConfig,

    #[serde(default)]
    pub command: CommandServerConfig,
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    log::info!("Starting HTTP service endpoint");

    let sender = DownstreamSender::new(KafkaSink::new("DOWNSTREAM_KAFKA_SINK")?)?;
    let commands = Commands::new();

    let config = Config::from_env()?;
    let max_payload_size = config.max_payload_size;
    let max_json_payload_size = config.max_json_payload_size;
    let http_server_commands = commands.clone();

    let device_authenticator = DeviceAuthenticator::new().await?;

    let http_server = HttpServer::new(move || {
        let app = App::new()
            .wrap(middleware::Logger::default())
            .app_data(web::PayloadConfig::new(max_payload_size))
            .data(web::JsonConfig::default().limit(max_json_payload_size))
            .data(sender.clone())
            .data(http_server_commands.clone());

        let app = app.app_data(Data::new(device_authenticator.clone()));

        app.service(index)
            // the standard endpoint
            .service(
                web::scope("/v1")
                    .service(
                        web::resource("/{channel}")
                            .route(web::post().to(telemetry::publish_plain::<KafkaSink>)),
                    )
                    .service(
                        web::resource("/{channel}/{suffix:.*}")
                            .route(web::post().to(telemetry::publish_tail::<KafkaSink>)),
                    ),
            )
            // The Things Network variant
            .service(
                web::scope("/ttn")
                    .route("/", web::post().to(ttn::publish_v2::<KafkaSink>))
                    .route("/v2", web::post().to(ttn::publish_v2::<KafkaSink>))
                    .route("/v3", web::post().to(ttn::publish_v3::<KafkaSink>)),
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

    let http_server = http_server.run();

    let mut command_server = CommandServer::new(config.command, commands.clone())?;

    // health server

    let health = HealthServer::new(config.health, vec![]);

    futures::try_join!(
        health.run(),
        command_server.deref_mut().err_into(),
        http_server.err_into()
    )?;

    Ok(())
}
