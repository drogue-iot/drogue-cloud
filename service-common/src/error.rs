use actix_web::{HttpResponse, ResponseError};
use drogue_client::error::ClientError;
use drogue_cloud_service_api::error::ErrorResponse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Error processing token")]
    TokenError,
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("Failed to authenticate")]
    AuthenticationError,
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Failed to serialize data: {0}")]
    Serializer(#[from] serde_json::Error),
    #[error("Resource not found: {0}/{1}")]
    NotFound(String, String),
    #[error("Client error: {0}")]
    Client(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl<E> From<ClientError<E>> for ServiceError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(err: ClientError<E>) -> Self {
        Self::Client(Box::new(err))
    }
}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::TokenError => HttpResponse::InternalServerError().json(ErrorResponse {
                error: "TokenError".into(),
                message: "Failed to decode token".into(),
            }),
            ServiceError::InternalError(message) => {
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
            ServiceError::InvalidRequest(message) => {
                HttpResponse::BadRequest().json(ErrorResponse {
                    error: "InvalidRequest".into(),
                    message: message.clone(),
                })
            }
            ServiceError::Serializer(err) => {
                HttpResponse::InternalServerError().json(ErrorResponse {
                    error: "Serializer".into(),
                    message: err.to_string(),
                })
            }
            ServiceError::NotFound(t, name) => HttpResponse::NotFound().json(ErrorResponse {
                error: "NotFound".into(),
                message: format!("Not found {0} / {1}", t, name),
            }),
            ServiceError::Client(err) => HttpResponse::ServiceUnavailable().json(ErrorResponse {
                error: "ClientError".into(),
                message: err.to_string(),
            }),
        }
    }
}
