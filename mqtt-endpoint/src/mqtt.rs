use ntex_mqtt::types::QoS;

use ntex_mqtt::v5::codec::{Auth, DisconnectReasonCode, PublishAckReason};
use ntex_mqtt::{v3, v5};

use drogue_cloud_endpoint_common::downstream::{
    DownstreamSender, Outcome, Publish, PublishResponse,
};

use crate::server::{ServerError, Session};
use std::fmt::Debug;

pub async fn connect_v3<Io>(
    connect: v3::Connect<Io>,
) -> Result<v3::ConnectAck<Io, Session>, ServerError> {
    log::info!("new connection: {:?}", connect);
    Ok(connect.ack(Session::new(DownstreamSender::new()?), false))
}

pub async fn connect_v5<Io>(
    connect: v5::Connect<Io>,
) -> Result<v5::ConnectAck<Io, Session>, ServerError> {
    log::info!("new connection: {:?}", connect);
    Ok(connect
        .ack(Session::new(DownstreamSender::new()?))
        .with(|ack| {
            ack.wildcard_subscription_available = Some(false);
        }))
}

pub async fn publish_v5(
    session: v5::Session<Session>,
    publish: v5::Publish,
) -> Result<v5::PublishAck, ServerError> {
    log::info!("incoming publish: {:?} - {:?}", publish, publish.packet());

    let channel = publish.topic().path();

    match session
        .state()
        .sender
        .publish(
            Publish {
                channel: channel.into(),
            },
            publish.payload(),
        )
        .await
    {
        Ok(PublishResponse {
            outcome: Outcome::Accepted,
        }) => Ok(publish.ack()),
        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => Ok(publish
            .ack()
            .reason_code(PublishAckReason::UnspecifiedError)),
        Err(e) => Err(ServerError { msg: e.to_string() }),
    }
}

pub async fn publish_v3(
    session: v3::Session<Session>,
    publish: v3::Publish,
) -> Result<(), ServerError> {
    log::info!(
        "incoming publish: {:?} -> {:?} / {:?}",
        publish.id(),
        publish.topic(),
        publish.packet(),
    );

    let channel = publish.topic().path();

    match session
        .state()
        .sender
        .publish(
            Publish {
                channel: channel.into(),
            },
            publish.payload(),
        )
        .await
    {
        Ok(PublishResponse {
            outcome: Outcome::Accepted,
        }) => Ok(()),

        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => Err(ServerError {
            // with MQTTv3, we can only close the connection
            msg: "Rejected".into(),
        }),

        Err(e) => Err(ServerError { msg: e.to_string() }),
    }
}

pub async fn control_v3(
    _: v3::Session<Session>,
    control: v3::ControlMessage,
) -> Result<v3::ControlResult, ServerError> {
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

pub async fn control_v5<E: Debug>(
    _: v5::Session<Session>,
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
