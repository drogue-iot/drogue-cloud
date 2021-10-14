use crate::{auth::AcceptAllClientCertVerifier, App, Config};
use anyhow::Context;
use async_trait::async_trait;
use drogue_client::registry;
use drogue_cloud_endpoint_common::{
    command::{Command, Commands},
    sender::{self, DownstreamSender, PublishOutcome, Publisher},
    sink::Sink,
};
use drogue_cloud_mqtt_common::{
    error::{PublishError, ServerError},
    mqtt::{self, *},
};
use drogue_cloud_service_common::Id;
use futures::future::ok;
use ntex::{
    fn_factory_with_config, fn_service,
    server::{rustls::Acceptor, ServerBuilder},
    util::{ByteString, Bytes},
};
use ntex_mqtt::{types::QoS, v3, v5, MqttError, MqttServer};
use ntex_service::pipeline_factory;
use pem::parse_many;
use rust_tls::{internal::pemfile::certs, PrivateKey, ServerConfig};
use std::{fs::File, io::BufReader, sync::Arc};

const TOPIC_COMMAND_INBOX: &str = "command/inbox";
const TOPIC_COMMAND_INBOX_PATTERN: &str = "command/inbox/#";

#[derive(Clone)]
pub struct Session<S>
where
    S: Sink,
{
    pub sender: DownstreamSender<S>,
    pub application: registry::v1::Application,
    pub device_id: Id,
    pub commands: Commands,
    sink: mqtt::Sink,
}

impl<S> Session<S>
where
    S: Sink,
{
    pub fn new(
        sender: DownstreamSender<S>,
        sink: mqtt::Sink,
        application: registry::v1::Application,
        device_id: Id,
        commands: Commands,
    ) -> Self {
        Self {
            sender,
            sink,
            application,
            device_id,
            commands,
        }
    }

    async fn run_commands(&self) {
        let device_id = self.device_id.clone();
        let mut rx = self.commands.subscribe(device_id.clone()).await;
        let sink = self.sink.clone();

        ntex::rt::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match Self::send_command(&sink, cmd).await {
                    Ok(_) => {
                        log::debug!("Command sent to device subscription {:?}", device_id);
                    }
                    Err(e) => {
                        log::error!("Failed to send a command to device subscription {:?}", e);
                    }
                }
            }
        });
    }

    async fn send_command(sink: &mqtt::Sink, cmd: Command) -> Result<(), String> {
        let topic = ByteString::from(format!("{}/{}", TOPIC_COMMAND_INBOX, cmd.command));

        let payload = match cmd.payload {
            Some(payload) => Bytes::from(payload),
            None => Bytes::new(),
        };

        match sink {
            mqtt::Sink::V3(sink) => match sink.publish(topic, payload).send_at_least_once().await {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
            mqtt::Sink::V5(sink) => match sink.publish(topic, payload).send_at_least_once().await {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        }
    }
}

#[async_trait(?Send)]
impl<S> mqtt::Session for Session<S>
where
    S: Sink,
{
    async fn publish(&self, publish: Publish<'_>) -> Result<(), PublishError> {
        let channel = publish.topic().path();
        let id = self.device_id.clone();

        match self
            .sender
            .publish(
                sender::Publish {
                    channel: channel.into(),
                    application: &self.application,
                    device_id: id.device_id,
                    options: Default::default(),
                },
                publish.payload(),
            )
            .await
        {
            Ok(PublishOutcome::Accepted) => Ok(()),
            Ok(PublishOutcome::Rejected) => Err(PublishError::UnspecifiedError),
            Ok(PublishOutcome::QueueFull) => Err(PublishError::QuotaExceeded),
            Err(err) => Err(PublishError::InternalError(err.to_string())),
        }
    }

    async fn subscribe(
        &self,
        sub: Subscribe<'_>,
    ) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        for mut sub in sub {
            if sub.topic() == TOPIC_COMMAND_INBOX_PATTERN {
                self.run_commands().await;
                log::debug!(
                    "Device '{:?}' subscribed to receive commands",
                    self.device_id
                );
                sub.confirm(QoS::AtLeastOnce);
            } else {
                log::info!("Subscribing to topic {:?} not allowed", sub.topic());
                sub.fail(v5::codec::SubscribeAckReason::UnspecifiedError);
            }
        }

        Ok(())
    }

    async fn unsubscribe(
        &self,
        _unsubscribe: Unsubscribe<'_>,
    ) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        // FIXME: any unsubscribe get we, we treat as disconnecting from the command inbox
        self.commands.unsubscribe(&self.device_id).await;
        Ok(())
    }

    async fn closed(&self) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        self.commands.unsubscribe(&self.device_id).await;
        Ok(())
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
            .map_err(|err| MqttError::Service(ServerError::InternalError(err.to_string())))
            .and_then(create_server!(app))
    })?)
}
