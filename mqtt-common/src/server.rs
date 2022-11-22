use crate::{error::ServerError, mqtt::*};
use futures::future::ok;
use ntex::{
    fn_service,
    http::{body, h1, HttpService, Request, Response, ResponseError},
    io::{Filter, Io},
    server::ServerBuilder,
    service::{fn_factory_with_config, pipeline_factory},
    time::Seconds,
    util::{
        variant::{variant, Variant2},
        Ready,
    },
    ws, ServiceFactory,
};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use serde::Deserialize;
use std::{fmt::Debug, time::Duration};

const DEFAULT_MAX_SIZE: u32 = 16 * 1024;

#[derive(Clone, Copy, Eq, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Mqtt,
    Websocket,
}

impl Default for Transport {
    fn default() -> Self {
        Self::Mqtt
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MqttServerOptions {
    #[serde(default)]
    pub max_size: Option<u32>,
    #[serde(default)]
    pub bind_addr: Option<String>,

    #[serde(default)]
    pub transport: Transport,

    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub handshake_timeout: Option<Duration>,

    #[serde(default)]
    pub workers: Option<usize>,
}

/// Create an new MQTT server
fn create_server<F, Svc, S>(
    opts: &MqttServerOptions,
    app: Svc,
) -> impl ServiceFactory<Io<F>, Response = (), InitError = (), Error = MqttError<ServerError>>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: Session + 'static,
    F: Filter,
{
    let transport = opts.transport;
    pipeline_factory(move |req: Io<_>| async move {
        Ok(match transport {
            Transport::Mqtt => Variant2::V1(req),
            Transport::Websocket => Variant2::V2(req),
        })
    })
    .and_then(variant(create_server_mqtt(opts, app.clone())).v2(create_server_ws(opts, app)))
}

/// Create an new MQTT server
fn create_server_mqtt<F, Svc, S>(
    opts: &MqttServerOptions,
    app: Svc,
) -> impl ServiceFactory<Io<F>, Response = (), InitError = (), Error = MqttError<ServerError>>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: Session + 'static,
    F: Filter,
{
    let app3 = app.clone();

    let max_size = opts.max_size.unwrap_or(DEFAULT_MAX_SIZE);

    MqttServer::new()
        .handshake_timeout(
            opts.handshake_timeout
                .map(|s| Seconds(s.as_secs() as u16))
                .unwrap_or_else(|| Seconds(15)),
        )
        // MQTTv3
        .v3(v3::MqttServer::new(fn_factory_with_config(move |_| {
            let app = app3.clone();
            ok::<_, ()>(fn_service(move |req| connect_v3(req, app.clone())))
        }))
        .max_size(max_size)
        .control(fn_factory_with_config(|session: v3::Session<S>| {
            ok::<_, ServerError>(fn_service(move |req| control_v3(session.clone(), req)))
        }))
        .publish(fn_factory_with_config(|session: v3::Session<S>| {
            ok::<_, ServerError>(fn_service(move |req| publish_v3(session.clone(), req)))
        })))
        // MQTTv5
        .v5(v5::MqttServer::new(fn_factory_with_config(move |_| {
            let app = app.clone();
            ok::<_, ()>(fn_service(move |req| connect_v5(req, app.clone())))
        }))
        .max_size(max_size)
        .control(fn_factory_with_config(|session: v5::Session<S>| {
            ok::<_, ServerError>(fn_service(move |req| control_v5(session.clone(), req)))
        }))
        .publish(fn_factory_with_config(|session: v5::Session<S>| {
            ok::<_, ServerError>(fn_service(move |req| publish_v5(session.clone(), req)))
        })))
}

/// Create a new MQTT server, wrapped in a WebSockets transport
pub fn create_server_ws<F, Svc, S>(
    opts: &MqttServerOptions,
    app: Svc,
) -> impl ServiceFactory<Io<F>, Response = (), InitError = (), Error = MqttError<ServerError>>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: Session + 'static,
    F: Filter,
{
    HttpService::build()
        .upgrade(
            pipeline_factory(|(req, io, codec): (Request, Io<F>, h1::Codec)| async move {
                log::debug!("Processing MQTT/WS handshake: {:?}", req);
                match ws::handshake(req.head()) {
                    Err(e) => {
                        io.send(
                            h1::Message::Item((
                                e.error_response().drop_body(),
                                body::BodySize::None,
                            )),
                            &codec,
                        )
                        .await?;
                        Err(MqttError::ServerError("WebSocket handshake error"))
                    }
                    Ok(mut res) => {
                        io.encode(
                            h1::Message::Item((
                                res.header("sec-websocket-protocol", "mqtt")
                                    .finish()
                                    .drop_body(),
                                body::BodySize::None,
                            )),
                            &codec,
                        )?;
                        io.add_filter(ws::WsTransportFactory::new(ws::Codec::default()))
                            .await
                            .map_err(|err| {
                                log::info!("Unable to create WebSocket transport: {}", err);
                                MqttError::ServerError("Unable to create WebSocket transport")
                            })
                    }
                }
            })
            .and_then(create_server_mqtt(opts, app)),
        )
        .finish(|req| {
            log::debug!("Unhandled request: {:?}", req);
            Ready::Ok::<_, std::io::Error>(
                Response::NotFound().body("MQTT Web Socket protocol only"),
            )
        })
        .map_err(|e| {
            log::info!("HTTP WebSocket server error: {}", e);
            MqttError::ServerError("HTTP WebSocket server error")
        })
}

pub trait TlsConfig {
    fn is_disabled(&self) -> bool;
    fn disable_client_certs(&self) -> bool;
    fn disable_psk(&self) -> bool;

    #[cfg(feature = "rustls")]
    fn verifier_rustls(&self) -> std::sync::Arc<dyn rust_tls::server::ClientCertVerifier> {
        std::sync::Arc::new(rust_tls::server::NoClientAuth)
    }

    fn key_file(&self) -> Option<&str>;
    fn cert_bundle_file(&self) -> Option<&str>;
}

fn server_builder(opts: &MqttServerOptions) -> ServerBuilder {
    let builder = ServerBuilder::new();

    if let Some(workers) = opts.workers {
        builder.workers(workers)
    } else {
        builder
    }
}

fn bind_addr(addr: &Option<String>, default: &str, debug: &str) -> String {
    let addr = addr.clone().unwrap_or_else(|| default.into());
    log::info!("Starting {} server: {}", debug, addr);

    addr
}

pub fn build_server<Svc, S, F, R>(
    opts: MqttServerOptions,
    tls: bool,
    app: Svc,
    factory: F,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: Session + 'static,
    F: Fn(&MqttServerOptions, Svc) -> R + Send + Clone + 'static,
    R: ServiceFactory<Io>,
{
    let mut builder = server_builder(&opts);

    let (default, default_ws, debug) = match tls {
        true => ("127.0.0.1:8883", "127.0.0.1:443", "TLS"),
        false => ("127.0.0.1:1883", "127.0.0.1:80", "non-TLS"),
    };

    let addr = match opts.transport {
        Transport::Mqtt => bind_addr(&opts.bind_addr, default, &format!("MQTT ({})", debug)),
        Transport::Websocket => {
            bind_addr(&opts.bind_addr, default_ws, &format!("MQTT-WS ({})", debug))
        }
    };

    builder = builder.bind("mqtt", addr, move |_| factory(&opts, app.clone()))?;

    // return

    Ok(builder)
}

pub fn build_nontls<Svc, S>(opts: MqttServerOptions, app: Svc) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: Session + 'static,
{
    build_server(opts, false, app, move |opts, app| create_server(opts, app))
}

#[cfg(feature = "rustls")]
pub fn build_rustls<Svc, S>(
    opts: MqttServerOptions,
    app: Svc,
    tls_config: rust_tls::server::ServerConfig,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: Session + 'static,
{
    log::info!("TLS based on rustls");

    build_server(opts, true, app, move |opts, app| {
        pipeline_factory(
            ntex::tls::rustls::Acceptor::new(std::sync::Arc::new(tls_config.clone())).timeout(
                opts.handshake_timeout
                    .map(|s| Seconds(s.as_secs() as u16))
                    .unwrap_or(Seconds(15)),
            ),
        )
        .map_err(|err| {
            log::debug!("Connect error: {}", err);
            MqttError::Service(ServerError::InternalError(err.to_string()))
        })
        .and_then(create_server(opts, app))
    })
}

#[cfg(feature = "openssl")]
pub fn build_openssl<Svc, S>(
    opts: MqttServerOptions,
    app: Svc,
    tls_config: open_ssl::ssl::SslAcceptor,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: Session + 'static,
{
    log::info!("TLS based on openssl");
    build_server(opts, true, app, move |opts, app| {
        pipeline_factory(
            ntex::tls::openssl::Acceptor::new(tls_config.clone()).timeout(
                opts.handshake_timeout
                    .map(|s| Seconds(s.as_secs() as u16))
                    .unwrap_or(Seconds(15)),
            ),
        )
        .map_err(|err| {
            log::debug!("Connect error: {}", err);
            MqttError::Service(ServerError::InternalError(err.to_string()))
        })
        .and_then(create_server(opts, app))
    })
}

pub fn build<Svc, F, S>(
    opts: MqttServerOptions,
    app: Svc,
    config: &dyn TlsConfig,
    psk_verifier: Option<F>,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    F: Fn(Option<&[u8]>, &mut [u8]) -> Result<usize, std::io::Error> + Send + Sync + 'static,
    S: Session + 'static,
{
    log::info!("MQTT transport: {:?}", opts.transport);

    if config.is_disabled() {
        return build_nontls(opts, app);
    }

    log::info!(
        "Client certificates disabled: {}",
        config.disable_client_certs()
    );

    log::info!("PSK disabled: {}", config.disable_psk());

    if cfg!(feature = "rustls") {
        // with rustls
        #[cfg(feature = "rustls")]
        return build_rustls(opts, app, crate::tls::rustls_config(config)?);
    } else if cfg!(feature = "openssl") {
        // with openssl
        #[cfg(feature = "openssl")]
        return build_openssl(opts, app, crate::tls::openssl_config(config, psk_verifier)?);
    }

    // no implementation available
    anyhow::bail!("Requested TLS configuration, but no TLS implementation is present")
}
