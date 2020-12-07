use drogue_cloud_endpoint_common::error::EndpointError;
use ntex_mqtt::v5;

#[derive(Debug)]
pub struct ServerError {
    pub msg: String,
}

impl From<()> for ServerError {
    fn from(_: ()) -> Self {
        ServerError { msg: "()".into() }
    }
}

impl From<anyhow::Error> for ServerError {
    fn from(err: anyhow::Error) -> Self {
        ServerError {
            msg: err.to_string(),
        }
    }
}

impl From<EndpointError> for ServerError {
    fn from(err: EndpointError) -> Self {
        ServerError {
            msg: err.to_string(),
        }
    }
}

impl std::convert::TryFrom<ServerError> for v5::PublishAck {
    type Error = ServerError;

    fn try_from(err: ServerError) -> Result<Self, Self::Error> {
        Err(err)
    }
}
