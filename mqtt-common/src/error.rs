use ntex_mqtt::{v3, v5};

#[derive(Debug)]
pub enum ServerError {
    InternalError(String),
    UnsupportedOperation,
    AuthenticationFailed,
    NotAuthorized,
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

impl MqttResponse<v5::PublishAck, v5::PublishAck> for ServerError {
    fn ack(&self, ack: v5::PublishAck) -> v5::PublishAck {
        match self {
            Self::InternalError(msg) => ack
                .reason(msg.clone().into())
                .reason_code(v5::codec::PublishAckReason::UnspecifiedError),
            Self::UnsupportedOperation => ack
                .reason("Unsupported operation".into())
                .reason_code(v5::codec::PublishAckReason::ImplementationSpecificError),
            Self::AuthenticationFailed => ack
                .reason("Authentication failed".into())
                .reason_code(v5::codec::PublishAckReason::ImplementationSpecificError),
            Self::NotAuthorized => ack
                .reason("Not authorized".into())
                .reason_code(v5::codec::PublishAckReason::NotAuthorized),
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
