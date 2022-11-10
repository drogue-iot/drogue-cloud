use drogue_client::error::ClientError;
use drogue_cloud_service_api::webapp::{
    error::PayloadError, http::StatusCode, HttpResponse, ResponseError,
};
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;

#[derive(Debug, thiserror::Error)]
pub enum EndpointError {
    #[error("Invalid data format: {}", source)]
    InvalidFormat { source: Box<dyn std::error::Error> },
    #[error("Invalid data: {}", details)]
    InvalidRequest { details: String },
    #[error("Endpoint configuration error: {}", details)]
    ConfigurationError { details: String },
    /// The authentication process failed to evaluate an outcome.
    #[error("Failed to authenticate: {}", source)]
    AuthenticationServiceError { source: Box<dyn std::error::Error> },
    /// The authentication process successfully evaluated that the access is denied.
    #[error("Authentication failed")]
    AuthenticationError,
}

impl EndpointError {
    pub fn name(&self) -> &str {
        match self {
            EndpointError::InvalidFormat { .. } => "InvalidFormat",
            EndpointError::InvalidRequest { .. } => "InvalidRequest",
            EndpointError::ConfigurationError { .. } => "ConfigurationError",
            EndpointError::AuthenticationServiceError { .. } => "AuthenticationServiceError",
            EndpointError::AuthenticationError { .. } => "AuthenticationError",
        }
    }
}

impl From<ClientError> for EndpointError {
    fn from(err: ClientError) -> Self {
        Self::AuthenticationServiceError {
            source: Box::new(err),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub code: u16,
    pub error: String,
    pub message: String,
}

#[derive(Debug)]
pub struct HttpEndpointError(pub EndpointError);

impl core::fmt::Display for HttpEndpointError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ResponseError for HttpEndpointError {
    fn status_code(&self) -> StatusCode {
        match self.0 {
            EndpointError::InvalidFormat { .. } => StatusCode::BAD_REQUEST,
            EndpointError::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            EndpointError::ConfigurationError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            EndpointError::AuthenticationServiceError { .. } => StatusCode::SERVICE_UNAVAILABLE,
            EndpointError::AuthenticationError { .. } => StatusCode::FORBIDDEN,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status_code = self.status_code();
        let error_response = ErrorResponse {
            code: status_code.as_u16(),
            message: self.to_string(),
            error: self.0.name().into(),
        };
        HttpResponse::build(status_code).json(error_response)
    }
}

impl From<PayloadError> for HttpEndpointError {
    fn from(err: PayloadError) -> Self {
        HttpEndpointError(EndpointError::InvalidFormat {
            source: Box::new(err),
        })
    }
}

impl From<EndpointError> for HttpEndpointError {
    fn from(err: EndpointError) -> Self {
        HttpEndpointError(err)
    }
}
