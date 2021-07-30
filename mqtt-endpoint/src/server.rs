use crate::{
    auth::AcceptAllClientCertVerifier,
    error::ServerError,
    mqtt::{connect_v3, connect_v5, control_v3, control_v5, publish_v3, publish_v5},
    App, Config,
};
use anyhow::Context;
use drogue_client::registry;
use drogue_cloud_endpoint_common::{command::Commands, downstream::DownstreamSender, sink::Sink};
use drogue_cloud_service_common::Id;
use futures::future::ok;
use ntex::{
    fn_factory_with_config, fn_service,
    server::{rustls::Acceptor, ServerBuilder},
};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use ntex_service::pipeline_factory;
use pem::parse_many;
use rust_tls::{internal::pemfile::certs, PrivateKey, ServerConfig};
use std::{fs::File, io::BufReader, sync::Arc};

#[derive(Clone)]
pub struct Session<S>
where
    S: Sink,
{
    pub sender: DownstreamSender<S>,
    pub application: registry::v1::Application,
    pub device_id: Id,
    pub commands: Commands,
}

impl<S> Session<S>
where
    S: Sink,
{
    pub fn new(
        sender: DownstreamSender<S>,
        application: registry::v1::Application,
        device_id: Id,
        commands: Commands,
    ) -> Self {
        Session {
            sender,
            application,
            device_id,
            commands,
        }
    }
}

const DEFAULT_MAX_SIZE: u32 = 1024;

fn tls_config(config: &Config) -> anyhow::Result<ServerConfig> {
    // This seems dangerous, as we simply accept all client certificates. However,
    // we validate them later during the "connect" packet validation.
    let client_cert_verifier = Arc::new(AcceptAllClientCertVerifier);
    let mut tls_config = ServerConfig::new(client_cert_verifier);

    let key = config
        .key_file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS configuration error: Missing key file"))?;
    let cert = config
        .cert_bundle_file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS configuration error: Missing cert file"))?;

    let cert_file = &mut BufReader::new(File::open(cert).unwrap());
    let cert_chain = certs(cert_file).unwrap();

    let mut keys = Vec::new();

    let pems = std::fs::read(key)?;
    for pem in parse_many(pems) {
        if pem.tag.contains("PRIVATE KEY") {
            keys.push(PrivateKey(pem.contents));
        }
    }

    if keys.len() > 1 {
        anyhow::bail!(
            "TLS configuration error: Found too many keys in the key file - found: {}",
            keys.len()
        );
    }

    if let Some(key) = keys.pop() {
        tls_config
            .set_single_cert(cert_chain, key)
            .context("Failed to set TLS certificate")?;
    } else {
        anyhow::bail!("TLS configuration error: No key found in the key file")
    }

    Ok(tls_config)
}

macro_rules! create_server {
    ($app:expr) => {{
        let app3 = $app.clone();
        let app5 = $app.clone();

        MqttServer::new()
            // MQTTv3
            .v3(v3::MqttServer::new(fn_factory_with_config(move |_| {
                let app = app3.clone();
                ok::<_, ()>(fn_service(move |req| connect_v3(req, app.clone())))
            }))
            .control(fn_factory_with_config(
                |session: v3::Session<Session<S>>| {
                    ok::<_, ServerError>(fn_service(move |req| control_v3(session.clone(), req)))
                },
            ))
            .publish(fn_factory_with_config(
                |session: v3::Session<Session<S>>| {
                    ok::<_, ServerError>(fn_service(move |req| publish_v3(session.clone(), req)))
                },
            )))
            // MQTTv5
            .v5(v5::MqttServer::new(fn_factory_with_config(move |_| {
                let app = app5.clone();
                ok::<_, ()>(fn_service(move |req| connect_v5(req, app.clone())))
            }))
            .max_size(DEFAULT_MAX_SIZE)
            .control(fn_factory_with_config(
                |session: v5::Session<Session<S>>| {
                    ok::<_, ServerError>(fn_service(move |req| control_v5(session.clone(), req)))
                },
            ))
            .publish(fn_factory_with_config(
                |session: v5::Session<Session<S>>| {
                    ok::<_, ServerError>(fn_service(move |req| publish_v5(session.clone(), req)))
                },
            )))
    }};
}

pub fn build<S>(
    addr: Option<&str>,
    builder: ServerBuilder,
    app: App<S>,
) -> anyhow::Result<ServerBuilder>
where
    S: Sink,
{
    let addr = addr.unwrap_or("127.0.0.1:1883");
    log::info!("Starting MQTT (non-TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, move || create_server!(app))?)
}

pub fn build_tls<S>(
    addr: Option<&str>,
    builder: ServerBuilder,
    app: App<S>,
    config: &Config,
) -> anyhow::Result<ServerBuilder>
where
    S: Sink,
{
    let addr = addr.unwrap_or("127.0.0.1:8883");
    log::info!("Starting MQTT (TLS) server: {}", addr);

    let tls_acceptor = Acceptor::new(tls_config(config)?);

    Ok(builder.bind("mqtt", addr, move || {
        pipeline_factory(tls_acceptor.clone())
            .map_err(|err| {
                MqttError::Service(ServerError {
                    msg: err.to_string(),
                })
            })
            .and_then(create_server!(app))
    })?)
}
