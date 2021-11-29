use crate::{
    error::ServerError,
    mqtt::{self, *},
};
use drogue_cloud_endpoint_common::x509::ClientCertificateRetriever;
use futures::future::ok;
use ntex::{
    codec::{AsyncRead, AsyncWrite},
    fn_factory_with_config, fn_service,
    rt::net::TcpStream,
    server::{rustls::Acceptor, ServerBuilder},
    time::Seconds,
    ServiceFactory,
};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use ntex_service::pipeline_factory;
use rust_tls::NoClientAuth;
use serde::Deserialize;
use std::time::Duration;
use std::{fmt::Debug, sync::Arc};

const DEFAULT_MAX_SIZE: u32 = 16 * 1024;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MqttServerOptions {
    pub max_size: Option<u32>,
    pub bind_addr: Option<String>,
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub handshake_timeout: Option<Duration>,

    #[serde(default)]
    pub workers: Option<usize>
}

fn create_server<Svc, S, Io>(
    opts: &MqttServerOptions,
    app: Svc,
) -> impl ServiceFactory<InitError = (), Config = (), Error = MqttError<ServerError>, Request = Io>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
    Io: ClientCertificateRetriever + Unpin + AsyncRead + AsyncWrite + Send + Sync + Debug + 'static,
{
    let app3 = app.clone();

    let max_size = opts.max_size.unwrap_or(DEFAULT_MAX_SIZE);

    MqttServer::<Io, _, _, _, _>::new()
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

pub trait TlsConfig {
    fn is_disabled(&self) -> bool;

    fn verifier(&self) -> Arc<dyn rust_tls::ClientCertVerifier> {
        Arc::new(NoClientAuth)
    }

    fn key_file(&self) -> Option<&str>;
    fn cert_bundle_file(&self) -> Option<&str>;
}

pub fn build_nontls<Svc, S>(opts: MqttServerOptions, app: Svc) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    let builder = ServerBuilder::new();

    let builder = if let Some(workers) = opts.workers {
        builder.workers(workers)
    } else {
        builder
    };

    let addr = opts
        .bind_addr
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:1883".into());
    log::info!("Starting MQTT (non-TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, move || create_server(&opts, app.clone()))?)
}

pub fn build_rustls<Svc, S>(
    opts: MqttServerOptions,
    app: Svc,
    tls_acceptor: Acceptor<TcpStream>,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    let builder = ServerBuilder::new();

    let builder = if let Some(workers) = opts.workers {
        builder.workers(workers)
    } else {
        builder
    };

    let addr = opts
        .bind_addr
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:8883".into());
    log::info!("Starting MQTT (TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, move || {
        pipeline_factory(tls_acceptor.clone())
            .map_err(|err| {
                log::debug!("Connect error: {}", err);
                MqttError::Service(ServerError::InternalError(err.to_string()))
            })
            .and_then(create_server(&opts, app.clone()))
    })?)
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
        build_nontls(opts, app)
    } else {
        let acceptor = Acceptor::new(crate::tls::rustls_config(config)?);
        build_rustls(opts, app, acceptor)
    }
}
