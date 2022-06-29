//! Clients for services.

mod device_auth;
mod device_state;
mod registry_auth;
mod user_auth;

pub mod metrics;

pub use device_auth::*;
pub use device_state::*;
pub use registry_auth::*;
pub use user_auth::*;

use drogue_client::error::{ClientError, ErrorInformation};
use http::StatusCode;
use reqwest::Response;

pub(crate) async fn default_error<T>(
    code: StatusCode,
    response: Response,
) -> Result<T, ClientError> {
    match response.json::<ErrorInformation>().await {
        Ok(error) => {
            log::debug!("Service reported error ({}): {}", code, error);
            Err(ClientError::Service { code, error })
        }
        Err(err) => {
            log::debug!(
                "Service call failed ({}). Result couldn't be decoded: {:?}",
                code,
                err
            );
            Err(ClientError::Request(format!(
                "Failed to decode service error response: {}",
                err
            )))
        }
    }
}
