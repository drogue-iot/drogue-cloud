use std::fmt::Debug;
use std::fs::File;
use std::io::BufReader;

use ntex::server::rustls::Acceptor;
use ntex::server::ServerBuilder;
use ntex_mqtt::types::QoS;
use ntex_mqtt::v5::codec::{Auth, DisconnectReasonCode};
use ntex_mqtt::{v3, v5, MqttError, MqttServer};
use ntex_service::pipeline_factory;
use rust_tls::{
    internal::pemfile::certs, internal::pemfile::rsa_private_keys, NoClientAuth, ServerConfig,
};

#[derive(Clone)]
struct Session;

#[derive(Debug)]
struct ServerError;

impl From<()> for ServerError {
    fn from(_: ()) -> Self {
        ServerError
    }
}

impl std::convert::TryFrom<ServerError> for v5::PublishAck {
    type Error = ServerError;

    fn try_from(err: ServerError) -> Result<Self, Self::Error> {
        Err(err)
    }
}

async fn connect_v3<Io>(
    connect: v3::Connect<Io>,
) -> Result<v3::ConnectAck<Io, Session>, ServerError> {
    log::info!("new connection: {:?}", connect);
    Ok(connect.ack(Session, false))
}

async fn publish_v3(publish: v3::Publish) -> Result<(), ServerError> {
    log::info!(
        "incoming publish: {:?} -> {:?}",
        publish.id(),
        publish.topic()
    );
    Ok(())
}

async fn connect_v5<Io>(
    connect: v5::Connect<Io>,
) -> Result<v5::ConnectAck<Io, Session>, ServerError> {
    log::info!("new connection: {:?}", connect);
    Ok(connect.ack(Session).with(|ack| {
        ack.wildcard_subscription_available = Some(false);
    }))
}

async fn publish_v5(publish: v5::Publish) -> Result<v5::PublishAck, ServerError> {
    log::info!("incoming publish: {:?}", publish,);
    Ok(publish.ack())
}

async fn control_v3(control: v3::ControlMessage) -> Result<v3::ControlResult, ServerError> {
    match control {
        v3::ControlMessage::Ping(p) => Ok(p.ack()),
        v3::ControlMessage::Disconnect(d) => Ok(d.ack()),
        v3::ControlMessage::Subscribe(mut s) => {
            s.iter_mut().for_each(|mut sub| {
                sub.subscribe(QoS::AtLeastOnce);
            });
            Ok(s.ack())
        }
        v3::ControlMessage::Unsubscribe(u) => Ok(u.ack()),
        v3::ControlMessage::Closed(c) => Ok(c.ack()),
    }
}

async fn control_v5<E: Debug>(
    control: v5::ControlMessage<E>,
) -> Result<v5::ControlResult, ServerError> {
    // log::info!("Control message: {:?}", control);

    match control {
        v5::ControlMessage::Auth(a) => Ok(a.ack(Auth::default())),
        v5::ControlMessage::Error(e) => Ok(e.ack(DisconnectReasonCode::UnspecifiedError)),
        v5::ControlMessage::ProtocolError(pe) => Ok(pe.ack()),
        v5::ControlMessage::Ping(p) => Ok(p.ack()),
        v5::ControlMessage::Disconnect(d) => Ok(d.ack()),
        v5::ControlMessage::Subscribe(mut s) => {
            s.iter_mut().for_each(|mut sub| {
                sub.subscribe(QoS::AtLeastOnce);
            });
            Ok(s.ack())
        }
        v5::ControlMessage::Unsubscribe(u) => Ok(u.ack()),
        v5::ControlMessage::Closed(c) => Ok(c.ack()),
    }
}

const MAX_SIZE: u32 = 1024;

fn build(addr: Option<&str>, builder: ServerBuilder) -> anyhow::Result<ServerBuilder> {
    let addr = addr.unwrap_or("127.0.0.1:1883");
    log::info!("Starting MQTT (non-TLS) server: {}", addr);

    Ok(builder.bind("mqtt", addr, || {
        MqttServer::new()
            .v3(v3::MqttServer::new(connect_v3)
                .control(control_v3)
                .publish(publish_v3))
            .v5(v5::MqttServer::new(connect_v5)
                .max_size(MAX_SIZE)
                .control(control_v5)
                .publish(publish_v5))
    })?)
}

fn build_tls(addr: Option<&str>, builder: ServerBuilder) -> anyhow::Result<ServerBuilder> {
    let addr = addr.unwrap_or("127.0.0.1:8883");

    log::info!("Loading TLS material...");

    let mut tls_config = ServerConfig::new(NoClientAuth::new());

    let key = std::env::var("KEY_FILE").unwrap_or("./examples/key.pem".into());
    let cert = std::env::var("CERT_FILE").unwrap_or("./examples/cert.pem".into());

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
            .map_err(|_err| MqttError::Service(ServerError {}))
            .and_then(
                MqttServer::new()
                    .v3(v3::MqttServer::new(connect_v3)
                        .control(control_v3)
                        .publish(publish_v3))
                    .v5(v5::MqttServer::new(connect_v5)
                        .max_size(MAX_SIZE)
                        .control(control_v5)
                        .publish(publish_v5)),
            )
    })?)
}

#[ntex::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let builder = ntex::server::Server::build();

    let tls = !std::env::var_os("DISABLE_TLS")
        .map(|s| s == "true")
        .unwrap_or(false);

    let addr = std::env::var("BIND_ADDR").ok();
    let addr = addr.as_ref().map(|s| s.as_str());

    let builder = if tls {
        build_tls(addr, builder)?
    } else {
        build(addr, builder)?
    };

    Ok(builder.workers(1).run().await?)
}
