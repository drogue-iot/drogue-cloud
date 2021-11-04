use crate::{
    error::{MqttResponse, ServerError},
    service::{App, Session},
};
use drogue_cloud_endpoint_common::sink::Sink as DownstreamSink;
use ntex::router::Path;
use ntex::util::{ByteString, Bytes};
use ntex_mqtt::{
    types::QoS,
    v3,
    v5::{
        self,
        codec::{Auth, ConnectAckReason, DisconnectReasonCode},
    },
};
use std::{fmt::Debug, num::NonZeroU32};

pub async fn connect_v3<Io, S>(
    connect: v3::Handshake<Io>,
    app: App<S>,
) -> Result<v3::HandshakeAck<Io, Session<S>>, ServerError>
where
    S: DownstreamSink,
{
    match app.connect(Connect::V3(&connect)).await {
        Ok(session) => Ok(connect.ack(session, false)),
        Err(_) => Ok(connect.bad_username_or_pwd()),
    }
}

pub async fn connect_v5<Io, S>(
    connect: v5::Handshake<Io>,
    app: App<S>,
) -> Result<v5::HandshakeAck<Io, Session<S>>, ServerError>
where
    S: DownstreamSink,
{
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

pub async fn publish_v3<S>(
    session: v3::Session<Session<S>>,
    publish: v3::Publish,
) -> Result<(), ServerError>
where
    S: DownstreamSink,
{
    session.publish(Publish::V3(&publish)).await
}

pub async fn publish_v5<S>(
    session: v5::Session<Session<S>>,
    publish: v5::Publish,
) -> Result<v5::PublishAck, ServerError>
where
    S: DownstreamSink,
{
    match session.publish(Publish::V5(&publish)).await {
        Ok(_) => Ok(publish.ack()),
        Err(err) => Ok(err.ack(publish.ack())),
    }
}

pub async fn control_v3<S>(
    session: v3::Session<Session<S>>,
    control: v3::ControlMessage<ServerError>,
) -> Result<v3::ControlResult, ServerError>
where
    S: DownstreamSink,
{
    match control {
        v3::ControlMessage::Error(err) => Ok(err.ack()),
        v3::ControlMessage::ProtocolError(err) => Ok(err.ack()),
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

pub async fn control_v5<E: Debug, S>(
    session: v5::Session<Session<S>>,
    control: v5::ControlMessage<E>,
) -> Result<v5::ControlResult, ServerError>
where
    S: DownstreamSink,
{
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

#[derive(Clone, Debug)]
pub enum Sink {
    V3(v3::MqttSink),
    V5(v5::MqttSink),
}

impl Sink {
    pub fn close(&self) {
        match self {
            Self::V3(sink) => sink.close(),
            Self::V5(sink) => sink.close(),
        }
    }
}

pub enum Connect<'a, Io> {
    V3(&'a v3::Handshake<Io>),
    V5(&'a v5::Handshake<Io>),
}

impl<'a, Io> Connect<'a, Io> {
    /// Return "clean session" for v3 and "clean start" for v5.
    pub fn clean_session(&self) -> bool {
        match self {
            Self::V3(connect) => connect.packet().clean_session,
            Self::V5(connect) => connect.packet().clean_start,
        }
    }

    /// Return the MQTT sink.
    pub fn sink(&self) -> Sink {
        match self {
            Self::V3(connect) => Sink::V3(connect.sink()),
            Self::V5(connect) => Sink::V5(connect.sink()),
        }
    }

    pub fn credentials(&self) -> (Option<&ByteString>, Option<&Bytes>) {
        match self {
            Self::V3(connect) => (
                connect.packet().username.as_ref(),
                connect.packet().password.as_ref(),
            ),
            Self::V5(connect) => (
                connect.packet().username.as_ref(),
                connect.packet().password.as_ref(),
            ),
        }
    }

    pub fn client_id(&self) -> &ByteString {
        match self {
            Self::V3(connect) => &connect.packet().client_id,
            Self::V5(connect) => &connect.packet().client_id,
        }
    }
}

pub enum Publish<'a> {
    V3(&'a v3::Publish),
    V5(&'a v5::Publish),
}

impl<'a> Publish<'a> {
    pub fn topic(&self) -> &Path<ByteString> {
        match self {
            Self::V3(publish) => publish.topic(),
            Self::V5(publish) => publish.topic(),
        }
    }

    pub fn payload(&self) -> &Bytes {
        match self {
            Self::V3(publish) => publish.payload(),
            Self::V5(publish) => publish.payload(),
        }
    }
}

pub enum Subscribe<'a> {
    V3(&'a mut v3::control::Subscribe),
    V5(&'a mut v5::control::Subscribe),
}

impl<'a> Subscribe<'a> {
    pub fn user_properties(&self) -> Option<&v5::codec::UserProperties> {
        match self {
            Self::V3(_) => None,
            Self::V5(sub) => Some(&sub.packet().user_properties),
        }
    }
}

impl<'a> Subscribe<'a> {
    pub fn id(&self) -> Option<NonZeroU32> {
        match self {
            Self::V3(_) => None,
            Self::V5(sub) => sub.packet().id,
        }
    }
}

impl<'a> IntoIterator for Subscribe<'a> {
    type Item = Subscription<'a>;
    type IntoIter = SubscriptionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::V3(sub) => SubscriptionIter::V3(sub.iter_mut()),
            Self::V5(sub) => SubscriptionIter::V5(sub.iter_mut()),
        }
    }
}

pub enum SubscriptionIter<'a> {
    V3(v3::control::SubscribeIter<'a>),
    V5(v5::control::SubscribeIter<'a>),
}

impl<'a> Iterator for SubscriptionIter<'a> {
    type Item = Subscription<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::V3(iter) => iter.next().map(Subscription::V3),
            Self::V5(iter) => iter.next().map(Subscription::V5),
        }
    }
}

pub enum Subscription<'a> {
    V3(v3::control::Subscription<'a>),
    V5(v5::control::Subscription<'a>),
}

impl<'a> Subscription<'a> {
    pub fn topic(&self) -> &'a ByteString {
        match self {
            Subscription::V3(sub) => sub.topic(),
            Subscription::V5(sub) => sub.topic(),
        }
    }

    #[allow(dead_code)]
    pub fn qos(&self) -> QoS {
        match self {
            Subscription::V3(sub) => sub.qos(),
            Subscription::V5(sub) => sub.options().qos,
        }
    }

    pub fn fail(&mut self, reason: v5::codec::SubscribeAckReason) {
        match self {
            Subscription::V3(sub) => sub.fail(),
            Subscription::V5(sub) => sub.fail(reason),
        }
    }

    pub fn confirm(&mut self, qos: QoS) {
        match self {
            Subscription::V3(sub) => sub.confirm(qos),
            Subscription::V5(sub) => sub.confirm(qos),
        }
    }
}

pub enum Unsubscribe<'a> {
    V3(&'a v3::control::Unsubscribe),
    V5(&'a mut v5::control::Unsubscribe),
}

impl<'a> IntoIterator for Unsubscribe<'a> {
    type Item = Unsubscription<'a>;
    type IntoIter = UnsubscriptionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::V3(unsub) => {
                let mut topics = unsub.iter().collect::<Vec<_>>();
                topics.reverse();
                UnsubscriptionIter::V3(topics)
            }
            Self::V5(unsub) => UnsubscriptionIter::V5(unsub.iter_mut()),
        }
    }
}

pub enum UnsubscriptionIter<'a> {
    V3(Vec<&'a ByteString>),
    V5(v5::control::UnsubscribeIter<'a>),
}

impl<'a> Iterator for UnsubscriptionIter<'a> {
    type Item = Unsubscription<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::V3(iter) => iter.pop().map(Unsubscription::V3),
            Self::V5(iter) => iter.next().map(Unsubscription::V5),
        }
    }
}

pub enum Unsubscription<'a> {
    V3(&'a ByteString),
    V5(v5::control::UnsubscribeItem<'a>),
}

impl<'a> Unsubscription<'a> {
    pub fn topic(&self) -> &'a ByteString {
        match self {
            Self::V3(topic) => topic,
            Self::V5(unsub) => unsub.topic(),
        }
    }
}
