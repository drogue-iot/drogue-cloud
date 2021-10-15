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
    ServiceFactory,
};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use ntex_service::pipeline_factory;
use rust_tls::NoClientAuth;
use std::fmt::Debug;
use std::sync::Arc;

const DEFAULT_MAX_SIZE: u32 = 1024;

fn create_server<Svc, S, Io>(
    app: Svc,
) -> impl ServiceFactory<InitError = (), Config = (), Error = MqttError<ServerError>, Request = Io>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
    Io: ClientCertificateRetriever + Unpin + AsyncRead + AsyncWrite + Send + Sync + Debug + 'static,
{
    let app3 = app.clone();

    MqttServer::<Io, _, _, _, _>::new()
        // MQTTv3
        .v3(v3::MqttServer::new(fn_factory_with_config(move |_| {
            let app = app3.clone();
            ok::<_, ()>(fn_service(move |req| connect_v3(req, app.clone())))
        }))
        .max_size(DEFAULT_MAX_SIZE)
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
        .max_size(DEFAULT_MAX_SIZE)
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

pub fn build_nontls<Svc, S>(addr: Option<&str>, app: Svc) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    let builder = ServerBuilder::new();

    let addr = addr.unwrap_or("127.0.0.1:1883");
    log::info!("Starting MQTT (non-TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, move || create_server(app.clone()))?)
}

pub fn build_rustls<Svc, S>(
    addr: Option<&str>,
    app: Svc,
    tls_acceptor: Acceptor<TcpStream>,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    let builder = ServerBuilder::new();

    let addr = addr.unwrap_or("127.0.0.1:8883");
    log::info!("Starting MQTT (TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, move || {
        pipeline_factory(tls_acceptor.clone())
            .map_err(|err| MqttError::Service(ServerError::InternalError(err.to_string())))
            .and_then(create_server(app.clone()))
    })?)
}

pub fn build<Svc, S>(
    addr: Option<&str>,
    app: Svc,
    config: &dyn TlsConfig,
) -> anyhow::Result<ServerBuilder>
where
    Svc: Service<S> + Clone + Send + 'static,
    S: mqtt::Session + 'static,
{
    if config.is_disabled() {
        build_nontls(addr, app)
    } else {
        let acceptor = Acceptor::new(crate::tls::rustls_config(config)?);
        build_rustls(addr, app, acceptor)
    }
}
