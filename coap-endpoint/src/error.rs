use coap_lite::ResponseType;
use drogue_cloud_endpoint_common::error::EndpointError;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct CoapEndpointError(pub EndpointError);

impl CoapEndpointError {
    pub fn status_code(&self) -> ResponseType {
        match self.0 {
            EndpointError::InvalidFormat { .. } => ResponseType::BadRequest,
            EndpointError::InvalidRequest { .. } => ResponseType::BadRequest,
            EndpointError::ConfigurationError { .. } => ResponseType::InternalServerError,
            EndpointError::AuthenticationServiceError { .. } => ResponseType::ServiceUnavailable,
            EndpointError::AuthenticationError { .. } => ResponseType::Forbidden,
        }
    }
}

impl core::fmt::Display for CoapEndpointError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<EndpointError> for CoapEndpointError {
    fn from(err: EndpointError) -> Self {
        CoapEndpointError(err)
    }
}
