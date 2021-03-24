use crate::{
    error::{MqttResponse, ServerError},
    service::{App, Connect, Publish, Session, Subscribe, Unsubscribe},
};
use ntex_mqtt::{
    v3,
    v5::{
        self,
        codec::{Auth, ConnectAckReason, DisconnectReasonCode},
    },
};
use std::fmt::Debug;

pub async fn connect_v3<Io>(
    connect: v3::Handshake<Io>,
    app: App,
) -> Result<v3::HandshakeAck<Io, Session>, ServerError> {
    match app.connect(Connect::V3(&connect)).await {
        Ok(session) => Ok(connect.ack(session, false)),
        Err(_) => Ok(connect.bad_username_or_pwd()),
    }
}

pub async fn connect_v5<Io>(
    connect: v5::Handshake<Io>,
    app: App,
) -> Result<v5::HandshakeAck<Io, Session>, ServerError> {
    match app.connect(Connect::V5(&connect)).await {
        Ok(session) => Ok(connect.ack(session).with(|ack| {
            ack.retain_available = Some(false);
            ack.shared_subscription_available = Some(true);
            ack.subscription_identifiers_available = Some(true);
            ack.wildcard_subscription_available = Some(false);
        })),
        Err(_) => Ok(connect.failed(ConnectAckReason::BadUserNameOrPassword)),
    }
}

pub async fn publish_v3(
    session: v3::Session<Session>,
    publish: v3::Publish,
) -> Result<(), ServerError> {
    session.publish(Publish::V3(&publish)).await
}

pub async fn publish_v5(
    session: v5::Session<Session>,
    publish: v5::Publish,
) -> Result<v5::PublishAck, ServerError> {
    match session.publish(Publish::V5(&publish)).await {
        Ok(_) => Ok(publish.ack()),
        Err(err) => Ok(err.ack(publish.ack())),
    }
}

pub async fn control_v3(
    session: v3::Session<Session>,
    control: v3::ControlMessage,
) -> Result<v3::ControlResult, ServerError> {
    match control {
        v3::ControlMessage::Ping(p) => Ok(p.ack()),
        v3::ControlMessage::Disconnect(d) => Ok(d.ack()),
        v3::ControlMessage::Subscribe(mut s) => {
            session.subscribe(Subscribe::V3(&mut s)).await?;
            Ok(s.ack())
        }
        v3::ControlMessage::Unsubscribe(u) => {
            match session.unsubscribe(Unsubscribe::V3(&u)).await {
                Ok(_) => Ok(u.ack()),
                Err(err) => Err(err),
            }
        }
        v3::ControlMessage::Closed(c) => {
            session.closed().await?;
            Ok(c.ack())
        }
    }
}

pub async fn control_v5<E: Debug>(
    session: v5::Session<Session>,
    control: v5::ControlMessage<E>,
) -> Result<v5::ControlResult, ServerError> {
    match control {
        v5::ControlMessage::Auth(a) => {
            // we don't do extended authentication (yet?)
            Ok(a.ack(Auth::default()))
        }
        v5::ControlMessage::Error(e) => Ok(e.ack(DisconnectReasonCode::UnspecifiedError)),
        v5::ControlMessage::ProtocolError(pe) => Ok(pe.ack()),
        v5::ControlMessage::Ping(p) => Ok(p.ack()),
        v5::ControlMessage::Disconnect(d) => Ok(d.ack()),
        v5::ControlMessage::Subscribe(mut s) => {
            session.subscribe(Subscribe::V5(&mut s)).await?;
            Ok(s.ack())
        }
        v5::ControlMessage::Unsubscribe(mut u) => {
            session.unsubscribe(Unsubscribe::V5(&mut u)).await?;
            Ok(u.ack())
        }
        v5::ControlMessage::Closed(c) => {
            session.closed().await?;
            Ok(c.ack())
        }
    }
}
