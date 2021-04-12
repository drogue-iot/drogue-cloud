//! Clients for services.

mod device_auth;
mod registry;
mod user_auth;

pub use device_auth::*;
pub use registry::*;
pub use user_auth::*;

use crate::openid::OpenIdTokenProvider;
use drogue_cloud_service_api::auth::ClientError;
use reqwest::RequestBuilder;

#[derive(Clone, Debug, Default)]
pub struct Context {
    pub provided_token: Option<String>,
}

async fn inject_token(
    token_provider: Option<OpenIdTokenProvider>,
    builder: RequestBuilder,
    mut context: Context,
) -> Result<RequestBuilder, ClientError<reqwest::Error>> {
    if let Some(token) = context.provided_token.take() {
        Ok(builder.bearer_auth(token))
    } else if let Some(provider) = token_provider {
        let token = provider
            .provide_token()
            .await
            .map_err(|err| ClientError::Token(Box::new(err)))?;
        Ok(builder.bearer_auth(token.access_token))
    } else {
        Ok(builder)
    }
}
