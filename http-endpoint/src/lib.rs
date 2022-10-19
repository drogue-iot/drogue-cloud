mod command;
mod downstream;
mod telemetry;
mod ttn;
mod x509;

use actix_cors::Cors;
use actix_web::{web, HttpResponse, Responder};
use drogue_cloud_endpoint_common::{
    auth::{AuthConfig, DeviceAuthenticator},
    command::{Commands, KafkaCommandSource, KafkaCommandSourceConfig},
    psk::{set_ssl_identity, Identity, VerifiedIdentity},
    sender::{DownstreamSender, ExternalClientPoolConfig},
    sink::KafkaSink,
};
use drogue_cloud_service_api::auth::device::authn::PreSharedKeyOutcome;
use drogue_cloud_service_api::{
    kafka::KafkaClientConfig,
    webapp::{self as actix_web},
};
use drogue_cloud_service_common::{
    actix::http::{HttpBuilder, HttpConfig},
    app::{Startup, StartupExt},
    defaults,
    tls::TlsAuthConfig,
};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
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

    // default for bool is false
    #[serde(default)]
    pub cors_allow_any_origin: bool,
}

async fn index() -> impl Responder {
    HttpResponse::Ok()
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
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

    let disable_tls_psk: bool = config.http.disable_tls_psk;
    let mut tls_auth_config = TlsAuthConfig::default();
    if !disable_tls_psk {
        let auth = device_authenticator.clone();
        tls_auth_config.psk = Some(Box::new(move |ssl, identity, secret_mut| {
            let mut to_copy = 0;
            if let Some(Ok(identity)) = identity.map(|s| core::str::from_utf8(s)) {
                log::trace!("PSK auth for {:?}", identity);
                if let Ok(identity) = Identity::parse(identity) {
                    let auth = auth.clone();
                    let app = identity.application().to_string();
                    let device = identity.device().to_string();
                    // Block this thread waiting for a response.
                    let response = tokio::task::block_in_place(move || {
                        // Run a temporary executor for this request
                        futures::executor::block_on(
                            async move { auth.request_psk(app, device).await },
                        )
                    });

                    if let Ok(response) = response {
                        if let PreSharedKeyOutcome::Found { app, device, key } = response.outcome {
                            to_copy = std::cmp::min(key.key.len(), secret_mut.len());
                            secret_mut[..to_copy].copy_from_slice(&key.key[..to_copy]);
                            set_ssl_identity(
                                ssl,
                                VerifiedIdentity {
                                    application: app,
                                    device,
                                },
                            );
                        }
                    }
                }
            }
            Ok(to_copy)
        }));
    }

    let main = HttpBuilder::new(config.http, Some(startup.runtime_config()), move |cfg| {
        let mut cors = Cors::default()
            .allowed_methods(vec!["POST"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::CONTENT_TYPE,
            ])
            .max_age(3600);

        if config.cors_allow_any_origin {
            cors = cors.allow_any_origin();
        }

        cfg.app_data(web::Data::new(sender.clone()))
            .app_data(web::Data::new(http_server_commands.clone()))
            .app_data(web::Data::new(device_authenticator.clone()))
            .service(web::resource("/").route(web::get().to(index)))
            // the standard endpoint
            .service(
                web::scope("/v1")
                    .wrap(cors)
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
    .tls_auth_config(tls_auth_config)
    .on_connect(move |con, ext| {
        let (mut psk, cert) = x509::from_socket(con);

        // Disable PSK identity
        if disable_tls_psk {
            psk = None;
        }

        if let Some(cert) = cert {
            if !cert.0.is_empty() {
                log::debug!("Added {} client certificates", cert.0.len());
                ext.insert(cert);
            }
        }
        ext.insert(psk);
    })
    .run()?;

    // command source

    let command_source = KafkaCommandSource::new(
        commands,
        config.kafka_command_config,
        config.command_source_kafka,
    )?;

    // spawn

    startup.spawn(main);
    startup.check(command_source);

    // done

    Ok(())
}
