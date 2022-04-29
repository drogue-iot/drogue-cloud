use drogue_client::error::ErrorInformation;
use drogue_cloud_service_api::webapp::{HttpResponse, ResponseError};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("not initialized")]
    NotInitialized,
    #[error("internal error: {0}")]
    Internal(String),
    #[error("connection pool error: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),
    #[error("database error: {0}")]
    Database(#[from] tokio_postgres::Error),
}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            Self::NotInitialized => HttpResponse::PreconditionFailed().json(ErrorInformation {
                error: "NotInitialized".into(),
                message: self.to_string(),
            }),
            Self::Internal(_) => HttpResponse::InternalServerError().json(ErrorInformation {
                error: "Internal".into(),
                message: self.to_string(),
            }),
            Self::Pool(_) => HttpResponse::ServiceUnavailable().json(ErrorInformation {
                error: "Pool".into(),
                message: self.to_string(),
            }),
            Self::Database(_) => HttpResponse::ServiceUnavailable().json(ErrorInformation {
                error: "Database".into(),
                message: self.to_string(),
            }),
        }
    }
}
