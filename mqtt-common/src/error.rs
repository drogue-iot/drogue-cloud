use ntex::util::ByteString;
use ntex_mqtt::{v3, v5};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PublishError {
    #[error("internal error: {0}")]
    InternalError(String),
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("not authorized")]
    NotAuthorized,
    #[error("unsupported operation")]
    UnsupportedOperation,
    #[error("payload format invalid")]
    PayloadFormatInvalid,
    #[error("unspecified error")]
    UnspecifiedError,
    #[error("topic name invalid")]
    TopicNameInvalid,
    #[error("protocol error")]
    ProtocolError,
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("internal error: {0}")]
    InternalError(String),
    #[error("protocol error")]
    ProtocolError,
    #[error("unsupported operation")]
    UnsupportedOperation,
    #[error("authentication failed")]
    AuthenticationFailed,
    #[error("not authorized")]
    NotAuthorized,
    #[error("publish error {0}")]
    PublishError(PublishError),
    #[error("configuration error {0}")]
    Configuration(String),
    /// Failed to acquire state
    #[error("state error {0}")]
    StateError(String),
}

impl From<PublishError> for ServerError {
    fn from(err: PublishError) -> Self {
        ServerError::PublishError(err)
    }
}

impl TryFrom<ServerError> for v5::PublishAck {
    type Error = ServerError;

    fn try_from(err: ServerError) -> Result<Self, Self::Error> {
        Err(err)
    }
}

pub trait MqttResponse<F, T> {
    fn ack(&self, ack: F) -> T;
}

impl MqttResponse<v5::PublishAck, v5::PublishAck> for PublishError {
    fn ack(&self, ack: v5::PublishAck) -> v5::PublishAck {
        match self {
            Self::InternalError(msg) => ack
                .reason(msg.as_str().into())
                .reason_code(v5::codec::PublishAckReason::UnspecifiedError),
            Self::UnspecifiedError => ack
                .reason(ByteString::from_static("Unspecified error"))
                .reason_code(v5::codec::PublishAckReason::UnspecifiedError),
            Self::UnsupportedOperation => ack
                .reason(ByteString::from_static("Unsupported operation"))
                .reason_code(v5::codec::PublishAckReason::ImplementationSpecificError),
            Self::NotAuthorized => ack
                .reason(ByteString::from_static("Not authorized"))
                .reason_code(v5::codec::PublishAckReason::NotAuthorized),
            Self::QuotaExceeded => ack
                .reason(ByteString::from_static("Quota exceeded"))
                .reason_code(v5::codec::PublishAckReason::QuotaExceeded),
            Self::PayloadFormatInvalid => ack
                .reason(ByteString::from_static("Payload format invalid"))
                .reason_code(v5::codec::PublishAckReason::PayloadFormatInvalid),
            Self::TopicNameInvalid => ack
                .reason(ByteString::from_static("Topic name is invalid"))
                .reason_code(v5::codec::PublishAckReason::TopicNameInvalid),
            Self::ProtocolError => ack
                .reason(ByteString::from_static("Protocol error"))
                .reason_code(v5::codec::PublishAckReason::UnspecifiedError),
        }
    }
}

impl<St> MqttResponse<v3::Handshake, v3::HandshakeAck<St>> for ServerError {
    fn ack(&self, ack: v3::Handshake) -> v3::HandshakeAck<St> {
        log::debug!("Handshake outcome (v3): {self}");

        match self {
            Self::AuthenticationFailed => ack.bad_username_or_pwd(),
            Self::NotAuthorized => ack.not_authorized(),
            _ => ack.service_unavailable(),
        }
    }
}

impl<St> MqttResponse<v5::Handshake, v5::HandshakeAck<St>> for ServerError {
    fn ack(&self, ack: v5::Handshake) -> v5::HandshakeAck<St> {
        log::debug!("Handshake outcome (v5): {self}");

        match self {
            Self::AuthenticationFailed => {
                ack.failed(v5::codec::ConnectAckReason::BadUserNameOrPassword)
            }
            Self::NotAuthorized => ack.failed(v5::codec::ConnectAckReason::NotAuthorized),
            _ => ack.failed(v5::codec::ConnectAckReason::UnspecifiedError),
        }
    }
}
