use actix_web::{HttpResponse, ResponseError};
use drogue_client::error::ErrorInformation;
use drogue_cloud_database_common::{error::ServiceError, models::GenerationError};
use drogue_cloud_registry_events::EventSenderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PostgresManagementServiceError<E>
where
    E: std::error::Error + std::fmt::Debug + 'static,
{
    #[error("Service error: {0}")]
    Service(#[from] ServiceError),
    #[error("Event sender error: {0}")]
    EventSender(#[from] EventSenderError<E>),
}

impl<E> From<tokio_postgres::Error> for PostgresManagementServiceError<E>
where
    E: std::error::Error + std::fmt::Debug + 'static,
{
    fn from(err: tokio_postgres::Error) -> Self {
        PostgresManagementServiceError::Service(err.into())
    }
}

impl<E> From<deadpool_postgres::PoolError> for PostgresManagementServiceError<E>
where
    E: std::error::Error + std::fmt::Debug + 'static,
{
    fn from(err: deadpool_postgres::PoolError) -> Self {
        PostgresManagementServiceError::Service(err.into())
    }
}

impl<E> From<GenerationError> for PostgresManagementServiceError<E>
where
    E: std::error::Error + std::fmt::Debug + 'static,
{
    fn from(err: GenerationError) -> Self {
        PostgresManagementServiceError::Service(err.into())
    }
}

impl<E> ResponseError for PostgresManagementServiceError<E>
where
    E: std::error::Error + std::fmt::Debug + 'static,
{
    fn error_response(&self) -> HttpResponse {
        match self {
            Self::Service(err) => err.error_response(),
            Self::EventSender(err) => HttpResponse::BadGateway().json(ErrorInformation {
                error: "EventSenderError".into(),
                message: err.to_string(),
            }),
        }
    }
}
