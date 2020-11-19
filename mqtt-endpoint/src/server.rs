use std::fmt::Debug;
use std::fs::File;
use std::io::BufReader;

use futures::future::ok;
use ntex::server::rustls::Acceptor;
use ntex::server::ServerBuilder;
use ntex::{fn_factory_with_config, fn_service};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use ntex_service::pipeline_factory;
use rust_tls::{
    internal::pemfile::certs, internal::pemfile::rsa_private_keys, NoClientAuth, ServerConfig,
};

use drogue_cloud_endpoint_common::downstream::DownstreamSender;

use crate::mqtt::{connect_v3, connect_v5, control_v3, control_v5, publish_v3, publish_v5};

#[derive(Clone)]
pub struct Session {
    pub sender: DownstreamSender,
    pub device_id: String,
}

impl Session {
    pub fn new(sender: DownstreamSender, device_id: String) -> Self {
        Session { sender, device_id }
    }
}

#[derive(Debug)]
pub struct ServerError {
    pub msg: String,
}

impl From<()> for ServerError {
    fn from(_: ()) -> Self {
        ServerError { msg: "()".into() }
    }
}

impl From<anyhow::Error> for ServerError {
    fn from(err: anyhow::Error) -> Self {
        ServerError {
            msg: err.to_string(),
        }
    }
}

impl std::convert::TryFrom<ServerError> for v5::PublishAck {
    type Error = ServerError;

    fn try_from(err: ServerError) -> Result<Self, Self::Error> {
        Err(err)
    }
}

const DEFAULT_MAX_SIZE: u32 = 1024;

pub fn build(addr: Option<&str>, builder: ServerBuilder) -> anyhow::Result<ServerBuilder> {
    let addr = addr.unwrap_or("127.0.0.1:1883");
    log::info!("Starting MQTT (non-TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, || {
        MqttServer::new()
            .v3(v3::MqttServer::new(connect_v3)
                .control(fn_factory_with_config(|session: v3::Session<Session>| {
                    ok::<_, ServerError>(fn_service(move |req| control_v3(session.clone(), req)))
                }))
                .publish(fn_factory_with_config(|session: v3::Session<Session>| {
                    ok::<_, ServerError>(fn_service(move |req| publish_v3(session.clone(), req)))
                })))
            .v5(v5::MqttServer::new(connect_v5)
                .max_size(DEFAULT_MAX_SIZE)
                .control(fn_factory_with_config(|session: v5::Session<Session>| {
                    ok::<_, ServerError>(fn_service(move |req| control_v5(session.clone(), req)))
                }))
                .publish(fn_factory_with_config(|session: v5::Session<Session>| {
                    ok::<_, ServerError>(fn_service(move |req| publish_v5(session.clone(), req)))
                })))
    })?)
}

pub fn build_tls(addr: Option<&str>, builder: ServerBuilder) -> anyhow::Result<ServerBuilder> {
    let addr = addr.unwrap_or("127.0.0.1:8883");

    log::info!("Loading TLS material... HAJ HO");

    let mut tls_config = ServerConfig::new(NoClientAuth::new());

    let key = std::env::var("KEY_FILE").unwrap_or_else(|_| "./examples/key.pem".into());
    let cert = std::env::var("CERT_FILE").unwrap_or_else(|_| "./examples/cert.pem".into());

    let cert_file = &mut BufReader::new(File::open(cert).unwrap());
    let key_file = &mut BufReader::new(File::open(key).unwrap());

    let cert_chain = certs(cert_file).unwrap();
    let mut keys = rsa_private_keys(key_file).unwrap();
    tls_config
        .set_single_cert(cert_chain, keys.remove(0))
        .unwrap();

    log::info!("Starting MQTT (TLS) server: {}", addr);

    let tls_acceptor = Acceptor::new(tls_config);

    Ok(builder.bind("mqtt", addr, move || {
        pipeline_factory(tls_acceptor.clone())
            .map_err(|err| {
                MqttError::Service(ServerError {
                    msg: err.to_string(),
                })
            })
            .and_then(
                MqttServer::new()
                    .v3(v3::MqttServer::new(connect_v3)
                        .control(fn_factory_with_config(|session: v3::Session<Session>| {
                            ok::<_, ServerError>(fn_service(move |req| {
                                control_v3(session.clone(), req)
                            }))
                        }))
                        .publish(fn_factory_with_config(|session: v3::Session<Session>| {
                            ok::<_, ServerError>(fn_service(move |req| {
                                publish_v3(session.clone(), req)
                            }))
                        })))
                    .v5(v5::MqttServer::new(connect_v5)
                        .max_size(DEFAULT_MAX_SIZE)
                        .control(fn_factory_with_config(|session: v5::Session<Session>| {
                            ok::<_, ServerError>(fn_service(move |req| {
                                control_v5(session.clone(), req)
                            }))
                        }))
                        .publish(fn_factory_with_config(|session: v5::Session<Session>| {
                            ok::<_, ServerError>(fn_service(move |req| {
                                publish_v5(session.clone(), req)
                            }))
                        }))),
            )
    })?)
}
