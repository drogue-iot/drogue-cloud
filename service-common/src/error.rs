use actix_web::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Error processing token")]
    TokenError,
    #[error("Internal error: {message}")]
    InternalError { message: String },
    #[error("Failed to authenticate")]
    AuthenticationError,
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
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
            ServiceError::ServiceUnavailable(message) => {
                HttpResponse::ServiceUnavailable().json(ErrorResponse {
                    error: "ServiceUnavailable".into(),
                    message: message.clone(),
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
