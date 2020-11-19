use actix_web::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum ServiceError {
    //    #[snafu(display("Invalid data format: {}", source))]
    //    InvalidFormat { source: Box<dyn std::error::Error> },
    #[snafu(display("Error processing token"))]
    TokenError,
}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::TokenError => HttpResponse::InternalServerError().json(ErrorResponse {
                error: "TokenError".into(),
                message: "Failed to decode token".into(),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
