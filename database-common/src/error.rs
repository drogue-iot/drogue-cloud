use actix_web::{HttpResponse, ResponseError};
use deadpool_postgres::PoolError;
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
    Conflict,
    #[error("Referenced a non-existing entity")]
    ReferenceNotFound,
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
                message: format!("{}", self),
            }),
            ServiceError::Database(..) => HttpResponse::ServiceUnavailable().json(ErrorResponse {
                error: "DatabaseError".into(),
                message: format!("{}", self),
            }),
            ServiceError::NotAuthorized => HttpResponse::Forbidden().json(ErrorResponse {
                error: "AuthenticationError".into(),
                message: format!("{}", self),
            }),
            ServiceError::NotFound => HttpResponse::NotFound().json(ErrorResponse {
                error: "NotFound".into(),
                message: format!("{}", self),
            }),
            ServiceError::Conflict => HttpResponse::Conflict().json(ErrorResponse {
                error: "Conflict".into(),
                message: format!("{}", self),
            }),
            ServiceError::ReferenceNotFound => HttpResponse::NotFound().json(ErrorResponse {
                error: "ReferenceNotFound".into(),
                message: format!("{}", self),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
