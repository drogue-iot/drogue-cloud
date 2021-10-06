use actix_http::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use drogue_cloud_service_common::error::ErrorResponse;
use keycloak::KeycloakError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::error::Error),
    #[error("Transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Client error: {0}")]
    Client(#[from] KeycloakError),
    #[error("Not authorized")]
    NotAuthorized,
    #[error("User not found")]
    NotFound,
}

impl Error {
    fn transport_error(err: &reqwest::Error) -> HttpResponse {
        HttpResponse::BadGateway().json(ErrorResponse {
            error: "GatewayError".into(),
            message: err.to_string(),
        })
    }
}

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        match self {
            Self::Internal(message) => HttpResponse::InternalServerError().json(ErrorResponse {
                error: "InternalError".into(),
                message: message.clone(),
            }),
            Self::Transport(err) => Self::transport_error(err),
            Self::Client(err) => match err {
                KeycloakError::ReqwestFailure(err) => Self::transport_error(err),
                KeycloakError::HttpFailure { status, body, text } => {
                    let mut resp = HttpResponse::build(
                        StatusCode::from_u16(*status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                    );
                    let error = body
                        .as_ref()
                        .and_then(|b| b.error.clone())
                        .unwrap_or_else(|| "UnknownError".into());
                    let message = body
                        .as_ref()
                        .and_then(|b| b.error_message.clone())
                        .unwrap_or_else(|| text.into());
                    resp.json(ErrorResponse { error, message })
                }
            },
            Self::NotAuthorized => HttpResponse::Forbidden().json(ErrorResponse {
                error: "NotAuthorized".into(),
                message: "Not authorized".into(),
            }),
            Self::Serialization(err) => HttpResponse::InternalServerError().json(ErrorResponse {
                error: "InternalError".into(),
                message: err.to_string(),
            }),
            Self::NotFound => HttpResponse::NotFound().json(ErrorResponse {
                error: "NotFound".into(),
                message: "User not found".into(),
            }),
        }
    }
}
