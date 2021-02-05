use crate::{
    auth::AcceptAllClientCertVerifier,
    error::ServerError,
    mqtt::{connect_v3, connect_v5, control_v3, control_v5, publish_v3, publish_v5},
    App, Config,
};
use anyhow::Context;
use drogue_cloud_endpoint_common::{downstream::DownstreamSender, Id};
use futures::future::ok;
use ntex::{
    fn_factory_with_config, fn_service,
    server::{rustls::Acceptor, ServerBuilder},
};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use ntex_service::pipeline_factory;
use rust_tls::{
    internal::pemfile::{certs, pkcs8_private_keys},
    ServerConfig,
};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::Sender;

#[derive(Clone)]
pub struct Session {
    pub sender: DownstreamSender,
    pub device_id: Id,
    pub devices: Arc<Mutex<HashMap<Id, Sender<String>>>>,
    pub tx: Sender<String>,
}

impl Session {
    pub fn new(
        sender: DownstreamSender,
        device_id: Id,
        devices: Arc<Mutex<HashMap<Id, Sender<String>>>>,
        tx: Sender<String>,
    ) -> Self {
        Session {
            sender,
            device_id,
            devices,
            tx,
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
        .cert_file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS configuration error: Missing cert file"))?;

    let cert_file = &mut BufReader::new(File::open(cert).unwrap());
    let key_file = &mut BufReader::new(File::open(key).unwrap());

    let cert_chain = certs(cert_file).unwrap();
    let mut keys = pkcs8_private_keys(key_file).unwrap();

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
            .control(fn_factory_with_config(|session: v3::Session<Session>| {
                ok::<_, ServerError>(fn_service(move |req| control_v3(session.clone(), req)))
            }))
            .publish(fn_factory_with_config(|session: v3::Session<Session>| {
                ok::<_, ServerError>(fn_service(move |req| publish_v3(session.clone(), req)))
            })))
            // MQTTv5
            .v5(v5::MqttServer::new(fn_factory_with_config(move |_| {
                let app = app5.clone();
                ok::<_, ()>(fn_service(move |req| connect_v5(req, app.clone())))
            }))
            .max_size(DEFAULT_MAX_SIZE)
            .control(fn_factory_with_config(|session: v5::Session<Session>| {
                ok::<_, ServerError>(fn_service(move |req| control_v5(session.clone(), req)))
            }))
            .publish(fn_factory_with_config(|session: v5::Session<Session>| {
                ok::<_, ServerError>(fn_service(move |req| publish_v5(session.clone(), req)))
            })))
    }};
}

pub fn build(
    addr: Option<&str>,
    builder: ServerBuilder,
    app: App,
) -> anyhow::Result<ServerBuilder> {
    let addr = addr.unwrap_or("127.0.0.1:1883");
    log::info!("Starting MQTT (non-TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, move || create_server!(app))?)
}

pub fn build_tls(
    addr: Option<&str>,
    builder: ServerBuilder,
    app: App,
    config: &Config,
) -> anyhow::Result<ServerBuilder> {
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
