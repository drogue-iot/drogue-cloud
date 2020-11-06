use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use drogue_cloud_endpoint_common::error::ErrorResponse;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct PusherError {
    pub code: StatusCode,
    pub error: String,
    pub message: String,
}

impl core::fmt::Display for PusherError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ResponseError for PusherError {
    fn status_code(&self) -> StatusCode {
        self.code
    }

    fn error_response(&self) -> HttpResponse {
        let status_code = self.status_code();
        let error_response = ErrorResponse {
            code: status_code.as_u16(),
            message: self.message.clone(),
            error: self.error.clone(),
        };
        HttpResponse::build(status_code).json(error_response)
    }
}
