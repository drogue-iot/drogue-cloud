use crate::{error::ServerError, server::Session, x509::ClientCertificateRetriever, App};
use bytes::Bytes;
use bytestring::ByteString;
use drogue_cloud_endpoint_common::downstream::{DownstreamSink, Publish, PublishOutcome};
use drogue_cloud_service_api::auth::device::authn::Outcome as AuthOutcome;
use drogue_cloud_service_common::Id;
use ntex_mqtt::{
    types::QoS,
    v3,
    v5::{
        self,
        codec::{Auth, ConnectAckReason, DisconnectReasonCode, PublishAckReason},
    },
};
use std::fmt::Debug;

const TOPIC_COMMAND_INBOX: &str = "command/inbox";
const TOPIC_COMMAND_INBOX_PATTERN: &str = "command/inbox/#";
// const TOPIC_COMMAND_OUTBOX: &str = "command/outbox";

macro_rules! connect {
    ($connect:expr, $app:expr, $certs:expr) => {{
        log::info!("new connection: {:?}", $connect);
        match $app
            .authenticate(
                &$connect.packet().username,
                &$connect.packet().password,
                &$connect.packet().client_id,
                $certs,
            )
            .await?
        {
            AuthOutcome::Pass {
                application,
                device,
                r#as: _,
            } => {
                let app_id = application.metadata.name.clone();
                let device_id = device.metadata.name.clone();

                let session = Session::new(
                    $app.downstream,
                    Id::new(app_id.clone(), device_id.clone()),
                    $app.commands.clone(),
                );

                Ok(session)
            }
            AuthOutcome::Fail => Err("Failed"),
        }
    }};
}

pub async fn connect_v3<Io, S>(
    mut connect: v3::Handshake<Io>,
    app: App<S>,
) -> Result<v3::HandshakeAck<Io, Session<S>>, ServerError>
where
    Io: ClientCertificateRetriever + 'static,
    S: DownstreamSink,
{
    let certs = connect.io().client_certs();
    log::debug!("Certs: {:?}", certs);

    // handle connect

    match connect!(connect, app, certs) {
        Ok(session) => Ok(connect.ack(session, false)),
        Err(_) => Ok(connect.bad_username_or_pwd()),
    }
}

pub async fn connect_v5<Io, S>(
    mut connect: v5::Handshake<Io>,
    app: App<S>,
) -> Result<v5::HandshakeAck<Io, Session<S>>, ServerError>
where
    Io: ClientCertificateRetriever + 'static,
    S: DownstreamSink,
{
    let certs = connect.io().client_certs();
    log::debug!("Certs: {:?}", certs);

    match connect!(connect, app, certs) {
        Ok(session) => Ok(connect.ack(session).with(|ack| {
            ack.wildcard_subscription_available = Some(false);
        })),
        Err(_) => Ok(connect.failed(ConnectAckReason::BadUserNameOrPassword)),
    }
}

macro_rules! publish {
    ($session: expr, $publish:expr) => {{
        log::debug!(
            "incoming publish: {:?} -> {:?} / {:?}",
            $publish.id(),
            $publish.topic(),
            $publish.packet(),
        );
        let channel = $publish.topic().path();

        let id = $session.device_id.clone();

        $session.state().sender.publish(
            Publish {
                channel: channel.into(),
                app_id: id.app_id,
                device_id: id.device_id,
                options: Default::default(),
            },
            $publish.payload(),
        )
    }};
}

pub async fn publish_v3<S>(
    session: v3::Session<Session<S>>,
    publish: v3::Publish,
) -> Result<(), ServerError>
where
    S: DownstreamSink,
{
    match publish!(session, publish).await {
        Ok(PublishOutcome::Accepted) => Ok(()),

        Ok(PublishOutcome::Rejected) => Err(ServerError {
            // with MQTTv3, we can only close the connection
            msg: "Rejected".into(),
        }),

        Ok(PublishOutcome::QueueFull) => Err(ServerError {
            // with MQTTv3, we can only close the connection
            msg: "QueueFull".into(),
        }),

        Err(e) => Err(ServerError { msg: e.to_string() }),
    }
}

pub async fn publish_v5<S>(
    session: v5::Session<Session<S>>,
    publish: v5::Publish,
) -> Result<v5::PublishAck, ServerError>
where
    S: DownstreamSink,
{
    match publish!(session, publish).await {
        Ok(PublishOutcome::Accepted) => Ok(publish.ack()),
        Ok(PublishOutcome::Rejected) => Ok(publish
            .ack()
            .reason_code(PublishAckReason::UnspecifiedError)),
        Ok(PublishOutcome::QueueFull) => {
            Ok(publish.ack().reason_code(PublishAckReason::QuotaExceeded))
        }
        Err(e) => Err(ServerError { msg: e.to_string() }),
    }
}

macro_rules! subscribe {
    ($s: expr, $session: expr, $fail: expr) => {{
        for mut sub in $s.iter_mut() {
            if sub.topic() == TOPIC_COMMAND_INBOX_PATTERN {
                let device_id = $session.state().device_id.clone();
                let mut rx = $session.state().commands.subscribe(device_id.clone()).await;
                let sink = $session.sink().clone();
                ntex::rt::spawn(async move {
                    while let Some(cmd) = rx.recv().await {
                        match sink
                            .publish(
                                ByteString::from(format!(
                                    "{}/{}",
                                    TOPIC_COMMAND_INBOX, cmd.command
                                )),
                                Bytes::from(cmd.payload.unwrap()),
                            )
                            .send_at_least_once()
                            .await
                        {
                            Ok(_) => {
                                log::debug!("Command sent to device subscription {:?}", device_id)
                            }
                            Err(e) => log::error!(
                                "Failed to send a command to device subscription {:?}",
                                e
                            ),
                        }
                    }
                });

                sub.subscribe(QoS::AtLeastOnce);

                log::debug!(
                    "Device '{:?}' subscribed to receive commands",
                    $session.state().device_id
                );
            } else {
                log::info!("Subscribing to topic {:?} not allowed", sub.topic());
                $fail(sub);
            }
        }

        Ok($s.ack())
    }};
}

macro_rules! unsubscribe {
    ($ack: expr, $session: expr, $log: expr) => {{
        $session
            .state()
            .commands
            .unsubscribe(&$session.state().device_id)
            .await;
        log::debug!($log, $session.state().device_id);
        Ok($ack.ack())
    }};
}

pub async fn control_v3<S>(
    session: v3::Session<Session<S>>,
    control: v3::ControlMessage,
) -> Result<v3::ControlResult, ServerError>
where
    S: DownstreamSink,
{
    match control {
        v3::ControlMessage::Ping(p) => Ok(p.ack()),
        v3::ControlMessage::Disconnect(d) => unsubscribe!(d, session, "Disconnecting device {:?}"),
        v3::ControlMessage::Subscribe(mut s) => {
            subscribe!(s, session, |mut sub: v3::control::Subscription| sub.fail())
        }
        v3::ControlMessage::Unsubscribe(u) => unsubscribe!(u, session, "Unsubscribing device {:?}"),
        v3::ControlMessage::Closed(c) => unsubscribe!(c, session, "Closing device connection {:?}"),
    }
}

pub async fn control_v5<E: Debug, S>(
    session: v5::Session<Session<S>>,
    control: v5::ControlMessage<E>,
) -> Result<v5::ControlResult, ServerError>
where
    S: DownstreamSink,
{
    match control {
        v5::ControlMessage::Auth(a) => Ok(a.ack(Auth::default())),
        v5::ControlMessage::Error(e) => Ok(e.ack(DisconnectReasonCode::UnspecifiedError)),
        v5::ControlMessage::ProtocolError(pe) => Ok(pe.ack()),
        v5::ControlMessage::Ping(p) => Ok(p.ack()),
        v5::ControlMessage::Disconnect(d) => unsubscribe!(d, session, "Disconnecting device {:?}"),
        v5::ControlMessage::Subscribe(mut s) => {
            subscribe!(s, session, |mut sub: v5::control::Subscription| sub
                .fail(v5::codec::SubscribeAckReason::NotAuthorized))
        }
        v5::ControlMessage::Unsubscribe(u) => unsubscribe!(u, session, "Unsubscribing device {:?}"),
        v5::ControlMessage::Closed(c) => unsubscribe!(c, session, "Closing device connection {:?}"),
    }
}
