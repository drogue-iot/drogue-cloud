use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum EndpointError {
    #[snafu(display("Invalid data format: {}", source))]
    InvalidFormat { source: Box<dyn std::error::Error> },
}

impl EndpointError {
    pub fn name(&self) -> &str {
        match self {
            EndpointError::InvalidFormat { .. } => "InvalidFormat",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub code: u16,
    pub error: String,
    pub message: String,
}
