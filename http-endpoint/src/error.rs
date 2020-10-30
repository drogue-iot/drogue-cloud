use actix_web::error::PayloadError;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use drogue_cloud_endpoint_common::error::{EndpointError, ErrorResponse};
use std::fmt::Formatter;

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
