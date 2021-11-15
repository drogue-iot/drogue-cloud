use ntex::util::ByteString;
use ntex_mqtt::{v3, v5};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    InternalError(String),
    QuotaExceeded,
    NotAuthorized,
    UnsupportedOperation,
    PayloadFormatInvalid,
    UnspecifiedError,
    TopicNameInvalid,
}

#[derive(Debug)]
pub enum ServerError {
    InternalError(String),
    UnsupportedOperation,
    AuthenticationFailed,
    NotAuthorized,
    PublishError(PublishError),
}

impl From<PublishError> for ServerError {
    fn from(err: PublishError) -> Self {
        ServerError::PublishError(err)
    }
}

impl std::convert::TryFrom<ServerError> for v5::PublishAck {
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
        }
    }
}

impl<Io, St> MqttResponse<v3::Handshake<Io>, v3::HandshakeAck<Io, St>> for ServerError {
    fn ack(&self, ack: v3::Handshake<Io>) -> v3::HandshakeAck<Io, St> {
        match self {
            Self::AuthenticationFailed => ack.bad_username_or_pwd(),
            Self::NotAuthorized => ack.not_authorized(),
            _ => ack.service_unavailable(),
        }
    }
}

impl<Io, St> MqttResponse<v5::Handshake<Io>, v5::HandshakeAck<Io, St>> for ServerError {
    fn ack(&self, ack: v5::Handshake<Io>) -> v5::HandshakeAck<Io, St> {
        match self {
            Self::AuthenticationFailed => {
                ack.failed(v5::codec::ConnectAckReason::BadUserNameOrPassword)
            }
            Self::NotAuthorized => ack.failed(v5::codec::ConnectAckReason::NotAuthorized),
            _ => ack.failed(v5::codec::ConnectAckReason::UnspecifiedError),
        }
    }
}
