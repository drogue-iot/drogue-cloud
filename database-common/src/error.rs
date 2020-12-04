use serde::{Deserialize, Serialize};
use snafu::Snafu;

use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};

#[derive(Debug, Snafu)]
pub enum ServiceError {
    #[snafu(display("Database error: {}", source))]
    DatabaseError { source: diesel::result::Error },
    #[snafu(display("Invalid database state"))]
    InvalidState,
    #[snafu(display("Conflict: {}", reason))]
    Conflict {
        reason: String,
        source: diesel::result::Error,
    },
}

impl ServiceError {
    pub fn name(&self) -> &str {
        match self {
            ServiceError::DatabaseError { .. } => "DatabaseError",
            ServiceError::InvalidState => "InvalidState",
            ServiceError::Conflict { .. } => "Conflict",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

impl ResponseError for ServiceError {
    fn status_code(&self) -> StatusCode {
        match self {
            ServiceError::DatabaseError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ServiceError::InvalidState => StatusCode::INTERNAL_SERVER_ERROR,
            ServiceError::Conflict { .. } => StatusCode::CONFLICT,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status_code = self.status_code();
        let error_response = ErrorResponse {
            message: self.to_string(),
            error: self.name().into(),
        };
        HttpResponse::build(status_code).json(error_response)
    }
}

impl From<diesel::result::Error> for ServiceError {
    fn from(source: diesel::result::Error) -> Self {
        match source {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            ) => ServiceError::Conflict {
                reason: "Duplicate item".into(),
                source,
            },
            _ => ServiceError::DatabaseError { source },
        }
    }
}
