use crate::models::GenerationError;
use deadpool_postgres::PoolError;
use drogue_cloud_service_api::webapp::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_postgres::error::SqlState;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Pool error: {0}")]
    Pool(#[from] PoolError),
    #[error("Database error: {0}")]
    Database(#[from] tokio_postgres::Error),
    #[error("Not authorized")]
    NotAuthorized,
    #[error("Not found")]
    NotFound,
    #[error("Conflict")]
    Conflict(String),
    #[error("Referenced a non-existing entity")]
    ReferenceNotFound,
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Lock failed")]
    OptimisticLockFailed,
}

impl From<GenerationError> for ServiceError {
    fn from(err: GenerationError) -> Self {
        ServiceError::Internal(err.to_string())
    }
}

impl ServiceError {
    /// return the underlying database error code, if there is one
    pub fn db_code(&self) -> Option<&str> {
        self.sql_state().map(|state| state.code())
    }

    /// return the underlying database error state, if there is one
    pub fn sql_state(&self) -> Option<&SqlState> {
        match self {
            ServiceError::Database(err) => err.code(),
            _ => None,
        }
    }
}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::Internal(message) => {
                HttpResponse::InternalServerError().json(ErrorResponse {
                    error: "InternalError".into(),
                    message: message.clone(),
                })
            }
            ServiceError::Pool(..) => HttpResponse::ServiceUnavailable().json(ErrorResponse {
                error: "PoolError".into(),
                message: self.to_string(),
            }),
            ServiceError::Database(..) => HttpResponse::ServiceUnavailable().json(ErrorResponse {
                error: "DatabaseError".into(),
                message: self.to_string(),
            }),
            ServiceError::NotAuthorized => HttpResponse::Forbidden().json(ErrorResponse {
                error: "AuthenticationError".into(),
                message: self.to_string(),
            }),
            ServiceError::NotFound => HttpResponse::NotFound().json(ErrorResponse {
                error: "NotFound".into(),
                message: self.to_string(),
            }),
            ServiceError::Conflict(_) => HttpResponse::Conflict().json(ErrorResponse {
                error: "Conflict".into(),
                message: self.to_string(),
            }),
            ServiceError::ReferenceNotFound => HttpResponse::NotFound().json(ErrorResponse {
                error: "ReferenceNotFound".into(),
                message: self.to_string(),
            }),
            ServiceError::BadRequest(_) => HttpResponse::BadRequest().json(ErrorResponse {
                error: "BadRequest".into(),
                message: self.to_string(),
            }),
            ServiceError::OptimisticLockFailed => HttpResponse::Conflict().json(ErrorResponse {
                error: "OptimisticLockFailed".into(),
                message: self.to_string(),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
