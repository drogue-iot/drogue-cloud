use actix_web::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum ServiceError {
    //    #[snafu(display("Invalid data format: {}", source))]
    //    InvalidFormat { source: Box<dyn std::error::Error> },
    #[snafu(display("Error processing token"))]
    TokenError,
    #[snafu(display("Internal error: {}", message))]
    InternalError { message: String },
    #[snafu(display("Failed to authenticate"))]
    AuthenticationError,
}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::TokenError => HttpResponse::InternalServerError().json(ErrorResponse {
                error: "TokenError".into(),
                message: "Failed to decode token".into(),
            }),
            ServiceError::InternalError { message } => {
                HttpResponse::InternalServerError().json(ErrorResponse {
                    error: "InternalError".into(),
                    message: message.clone(),
                })
            }
            ServiceError::AuthenticationError => HttpResponse::Forbidden().json(ErrorResponse {
                error: "AuthenticationError".into(),
                message: "Not authorized".into(),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
