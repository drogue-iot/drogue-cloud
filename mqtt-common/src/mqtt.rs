use crate::error::{MqttResponse, PublishError, ServerError};
use async_trait::async_trait;
use ntex::{
    io::IoBoxed,
    router::Path,
    util::{ByteString, Bytes},
};
use ntex_mqtt::{
    types::QoS,
    v3,
    v5::{
        self,
        codec::{Auth, ConnectAckReason, DisconnectReasonCode},
    },
};
use std::{fmt::Debug, io, num::NonZeroU32};

#[async_trait(?Send)]
pub trait Service<S>
where
    S: Session,
{
    async fn connect<'a>(&'a self, connect: Connect<'a>) -> Result<ConnectAck<S>, ServerError>;
}

pub type AckOptions = v5::codec::ConnectAck;

pub struct ConnectAck<S>
where
    S: Session,
{
    pub session: S,
    pub ack: AckOptions,
}

#[async_trait(?Send)]
pub trait Session {
    async fn publish(&self, publish: Publish<'_>) -> Result<(), PublishError>;
    async fn subscribe(&self, subscribe: Subscribe<'_>) -> Result<(), ServerError>;
    async fn unsubscribe(&self, unsubscribe: Unsubscribe<'_>) -> Result<(), ServerError>;
    async fn closed(&self, reason: CloseReason) -> Result<(), ServerError>;
}

#[derive(Debug)]
pub enum CloseReason {
    Closed { was_error: bool },
    PeerGone(Option<io::Error>),
}

pub async fn connect_v3<A, S>(
    mut connect: v3::Handshake,
    app: A,
) -> Result<v3::HandshakeAck<S>, ServerError>
where
    A: Service<S>,
    S: Session,
{
    match app.connect(Connect::V3(&mut connect)).await {
        Ok(ack) => Ok(connect.ack(ack.session, ack.ack.session_present)),
        Err(_) => Ok(connect.bad_username_or_pwd()),
    }
}

pub async fn connect_v5<A, S>(
    mut connect: v5::Handshake,
    app: A,
) -> Result<v5::HandshakeAck<S>, ServerError>
where
    A: Service<S>,
    S: Session,
{
    match app.connect(Connect::V5(&mut connect)).await {
        Ok(connect_ack) => Ok(connect.ack(connect_ack.session).with(|ack| {
            *ack = connect_ack.ack;
        })),
        Err(_) => Ok(connect.failed(ConnectAckReason::BadUserNameOrPassword)),
    }
}

pub async fn publish_v3<S>(session: v3::Session<S>, publish: v3::Publish) -> Result<(), ServerError>
where
    S: Session,
{
    // for v3, we ignore the publish error and return a server error
    session.publish(Publish::V3(&publish)).await?;
    Ok(())
}

pub async fn publish_v5<S>(
    session: v5::Session<S>,
    publish: v5::Publish,
) -> Result<v5::PublishAck, ServerError>
where
    S: Session,
{
    match session.publish(Publish::V5(&publish)).await {
        Ok(_) => Ok(publish.ack()),
        Err(err) => Ok(err.ack(publish.ack())),
    }
}

pub async fn control_v3<S>(
    session: v3::Session<S>,
    control: v3::ControlMessage<ServerError>,
) -> Result<v3::ControlResult, ServerError>
where
    S: Session,
{
    match control {
        v3::ControlMessage::Error(err) => Ok(err.ack()),
        v3::ControlMessage::ProtocolError(err) => Ok(err.ack()),
        v3::ControlMessage::Ping(p) => Ok(p.ack()),
        v3::ControlMessage::Disconnect(d) => Ok(d.ack()),
        v3::ControlMessage::PeerGone(mut g) => {
            session.closed(CloseReason::PeerGone(g.take())).await?;
            Ok(g.ack())
        }
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
            session
                .closed(CloseReason::Closed {
                    was_error: c.is_error(),
                })
                .await?;
            Ok(c.ack())
        }
    }
}

pub async fn control_v5<E: Debug, S>(
    session: v5::Session<S>,
    control: v5::ControlMessage<E>,
) -> Result<v5::ControlResult, ServerError>
where
    S: Session,
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
        v5::ControlMessage::PeerGone(mut g) => {
            session.closed(CloseReason::PeerGone(g.take())).await?;
            Ok(g.ack())
        }
        v5::ControlMessage::Subscribe(mut s) => {
            session.subscribe(Subscribe::V5(&mut s)).await?;
            Ok(s.ack())
        }
        v5::ControlMessage::Unsubscribe(mut u) => {
            session.unsubscribe(Unsubscribe::V5(&mut u)).await?;
            Ok(u.ack())
        }
        v5::ControlMessage::Closed(c) => {
            session
                .closed(CloseReason::Closed {
                    was_error: c.is_error(),
                })
                .await?;
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

impl From<v3::MqttSink> for Sink {
    fn from(sink: v3::MqttSink) -> Self {
        Self::V3(sink)
    }
}

impl From<v5::MqttSink> for Sink {
    fn from(sink: v5::MqttSink) -> Self {
        Self::V5(sink)
    }
}

#[derive(Debug)]
pub enum Connect<'a> {
    V3(&'a mut v3::Handshake),
    V5(&'a mut v5::Handshake),
}

impl<'a> Connect<'a> {
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

    pub fn io(&mut self) -> &IoBoxed {
        match self {
            Self::V3(connect) => connect.io(),
            Self::V5(connect) => connect.io(),
        }
    }
}

#[derive(Debug)]
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

    pub fn properties(&self) -> Option<&v5::codec::PublishProperties> {
        match self {
            Self::V3(_) => None,
            Self::V5(publish) => Some(&publish.packet().properties),
        }
    }

    pub fn qos(&self) -> QoS {
        match self {
            Self::V3(publish) => publish.qos(),
            Self::V5(publish) => publish.qos(),
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum Subscription<'a> {
    V3(v3::control::Subscription<'a>),
    V5(v5::control::Subscription<'a>),
}

impl<'a> Subscription<'a> {
    pub fn topic(&self) -> &'a ByteString {
        match self {
            Self::V3(sub) => sub.topic(),
            Self::V5(sub) => sub.topic(),
        }
    }

    #[allow(dead_code)]
    pub fn qos(&self) -> QoS {
        match self {
            Self::V3(sub) => sub.qos(),
            Self::V5(sub) => sub.options().qos,
        }
    }

    pub fn fail(&mut self, reason: v5::codec::SubscribeAckReason) {
        match self {
            Self::V3(sub) => sub.fail(),
            Self::V5(sub) => sub.fail(reason),
        }
    }

    pub fn confirm(&mut self, qos: QoS) {
        match self {
            Self::V3(sub) => sub.confirm(qos),
            Self::V5(sub) => sub.confirm(qos),
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
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

    pub fn success(&mut self) {
        match self {
            Self::V3(_) => {}
            Self::V5(unsub) => unsub.success(),
        }
    }

    pub fn fail(&mut self, reason: v5::codec::UnsubscribeAckReason) {
        match self {
            Self::V3(_) => {}
            Self::V5(unsub) => unsub.fail(reason),
        }
    }
}
