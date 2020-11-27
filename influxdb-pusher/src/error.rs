use actix_web::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum ServiceError {
    #[snafu(display("Error processing JSON path: {details}", details=details))]
    SelectorError { details: String },
    #[snafu(display("Failed processing payload: {details}", details=details))]
    PayloadParseError { details: String },
}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        let message = format!("{}", self);
        match self {
            ServiceError::SelectorError { .. } => {
                HttpResponse::NotAcceptable().json(ErrorResponse {
                    error: "SelectorError".into(),
                    message,
                })
            }
            ServiceError::PayloadParseError { .. } => {
                HttpResponse::NotAcceptable().json(ErrorResponse {
                    error: "PayloadError".into(),
                    message,
                })
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
