use crate::{
    error::ServerError,
    mqtt::{self, *},
};
use futures::future::ok;
use ntex::{
    fn_service,
    http::{body, h1, HttpService, Request, Response, ResponseError},
    io::{Filter, Io},
    server::ServerBuilder,
    service::{fn_factory_with_config, pipeline_factory},
    time::Seconds,
    util::Ready,
    ws, ServiceFactory,
};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use serde::Deserialize;
use std::{fmt::Debug, time::Duration};

const DEFAULT_MAX_SIZE: u32 = 16 * 1024;

pub enum Transport {
    Mqtt,
    Websocket,
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MqttServerOptions {
    #[serde(default)]
    pub max_size: Option<u32>,
    #[serde(default)]
    pub bind_addr: Option<String>,

    #[serde(default)]
    pub bind_addr_ws: Option<String>,
    #[serde(default)]
    pub disable_ws: bool,

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
    S: mqtt::Session + 'static,
    F: Filter,
{
    let app3 = app.clone();

    let max_size = opts.max_size.unwrap_or(DEFAULT_MAX_SIZE);

    MqttServer::new()
        .handshake_timeout(Seconds(15))
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
    S: mqtt::Session + 'static,
    F: Filter,
{
    HttpService::build()
        .on_request(move |(req, _io)| async {
            log::debug!("Request: {:?}", req);
            Ok(req)
        })
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
            .and_then(create_server(opts, app)),
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

pub fn build_server<Svc, S, F1, R1, F2, R2>(
    opts: MqttServerOptions,
    tls: bool,
    app: Svc,
    factory: F1,
    factory_ws: F2,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
    F1: Fn(&MqttServerOptions, Svc) -> R1 + Send + Clone + 'static,
    R1: ServiceFactory<Io>,
    F2: Fn(&MqttServerOptions, Svc) -> R2 + Send + Clone + 'static,
    R2: ServiceFactory<Io>,
{
    let mut builder = server_builder(&opts);

    let (default, default_ws, debug) = match tls {
        true => ("127.0.0.1:8883", "127.0.0.1:443", "TLS"),
        false => ("127.0.0.1:1883", "127.0.0.1:80", "non-TLS"),
    };

    let addr = bind_addr(&opts.bind_addr, default, &format!("MQTT ({})", debug));

    let addr_ws = bind_addr(
        &opts.bind_addr_ws,
        default_ws,
        &format!("MQTT-WS ({})", debug),
    );
    let opts_ws = opts.clone();
    let app_ws = app.clone();

    // enable WebSockets server

    if !opts.disable_ws {
        builder = builder.bind("mqtt-ws", addr_ws, move |_| {
            factory_ws(&opts, app_ws.clone())
        })?;
    }

    // enable plain MQTT server

    builder = builder.bind("mqtt", addr, move |_| factory(&opts_ws, app.clone()))?;

    // return

    Ok(builder)
}

pub fn build_nontls<Svc, S>(opts: MqttServerOptions, app: Svc) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    build_server(
        opts,
        false,
        app,
        move |opts, app| create_server(opts, app),
        move |opts, app| create_server_ws(opts, app),
    )
}

#[cfg(feature = "rustls")]
pub fn build_rustls<Svc, S>(
    opts: MqttServerOptions,
    app: Svc,
    tls_config: std::sync::Arc<rust_tls::server::ServerConfig>,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    log::info!("TLS based on rustls");

    let tls_config_ws = tls_config.clone();

    build_server(
        opts,
        true,
        app,
        move |opts, app| {
            pipeline_factory(ntex::tls::rustls::Acceptor::new(tls_config.clone()))
                .map_err(|err| {
                    log::debug!("Connect error: {}", err);
                    MqttError::Service(ServerError::InternalError(err.to_string()))
                })
                .and_then(create_server(opts, app))
        },
        move |opts, app| {
            pipeline_factory(ntex::tls::rustls::Acceptor::new(tls_config_ws.clone()))
                .map_err(|err| {
                    log::debug!("Connect error: {}", err);
                    MqttError::Service(ServerError::InternalError(err.to_string()))
                })
                .and_then(create_server_ws(opts, app))
        },
    )
}

#[cfg(feature = "openssl")]
pub fn build_openssl<Svc, S>(
    opts: MqttServerOptions,
    app: Svc,
    tls_config: open_ssl::ssl::SslAcceptor,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    log::info!("TLS based on openssl");

    let tls_config_ws = tls_config.clone();

    build_server(
        opts,
        true,
        app,
        move |opts, app| {
            pipeline_factory(ntex::tls::openssl::Acceptor::new(tls_config.clone()))
                .map_err(|err| {
                    log::debug!("Connect error: {}", err);
                    MqttError::Service(ServerError::InternalError(err.to_string()))
                })
                .and_then(create_server(opts, app))
        },
        move |opts, app| {
            pipeline_factory(ntex::tls::openssl::Acceptor::new(tls_config_ws.clone()))
                .map_err(|err| {
                    log::debug!("Connect error: {}", err);
                    MqttError::Service(ServerError::InternalError(err.to_string()))
                })
                .and_then(create_server_ws(opts, app))
        },
    )
}

pub fn build<Svc, S>(
    opts: MqttServerOptions,
    app: Svc,
    config: &dyn TlsConfig,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    if config.is_disabled() {
        return build_nontls(opts, app);
    }

    if cfg!(feature = "rustls") {
        // with rustls
        #[cfg(feature = "rustls")]
        return build_rustls(
            opts,
            app,
            std::sync::Arc::new(crate::tls::rustls_config(config)?),
        );
    } else if cfg!(feature = "openssl") {
        // with openssl
        #[cfg(feature = "openssl")]
        return build_openssl(opts, app, crate::tls::openssl_config(config)?);
    }

    // no implementation available
    anyhow::bail!("Requested TLS configuration, but no TLS implementation is present")
}
