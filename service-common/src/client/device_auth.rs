use drogue_client::{
    core::WithTracing,
    error::{ClientError, ErrorInformation},
    openid::{OpenIdTokenProvider, TokenInjector},
};
use drogue_cloud_service_api::auth::device::authn::{
    AuthenticationRequest, AuthenticationResponse, AuthorizeGatewayRequest,
    AuthorizeGatewayResponse,
};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use tracing::instrument;
use url::Url;

/// An authentication client backed by reqwest.
#[derive(Clone, Debug)]
pub struct ReqwestAuthenticatorClient {
    client: Client,
    auth_service_url: Url,
    auth_as_url: Url,
    token_provider: Option<OpenIdTokenProvider>,
}

impl ReqwestAuthenticatorClient {
    /// Create a new client instance.
    pub fn new(
        client: Client,
        url: Url,
        token_provider: Option<OpenIdTokenProvider>,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            client,
            auth_service_url: url.join("auth")?,
            auth_as_url: url.join("authorize_as")?,
            token_provider,
        })
    }

    #[instrument]
    pub async fn authenticate(
        &self,
        request: AuthenticationRequest,
    ) -> Result<AuthenticationResponse, ClientError<reqwest::Error>> {
        self.request(self.auth_service_url.clone(), request).await
    }

    #[instrument]
    pub async fn authorize_as(
        &self,
        request: AuthorizeGatewayRequest,
    ) -> Result<AuthorizeGatewayResponse, ClientError<reqwest::Error>> {
        self.request(self.auth_as_url.clone(), request).await
    }

    async fn request<T, U>(&self, url: Url, request: T) -> Result<U, ClientError<reqwest::Error>>
    where
        T: Debug + Serialize,
        for<'de> U: Debug + Deserialize<'de>,
    {
        let req = self
            .client
            .post(url)
            .propagate_current_context()
            .inject_token(&self.token_provider)
            .await?;

        let response: Response = req.json(&request).send().await.map_err(|err| {
            log::warn!("Request error {:?}: {}", request, err);
            Box::new(err)
        })?;

        match response.status() {
            StatusCode::OK => match response.json::<U>().await {
                Ok(result) => {
                    log::debug!("Outcome for {:?} is {:?}", request, result);
                    Ok(result)
                }
                Err(err) => {
                    log::debug!("Request failed for {:?}. Result: {:?}", request, err);

                    Err(ClientError::Request(format!(
                        "Failed to decode service response: {}",
                        err
                    )))
                }
            },
            code => match response.json::<ErrorInformation>().await {
                Ok(result) => {
                    log::debug!("Service reported error ({}): {}", code, result);
                    Err(ClientError::Service(result))
                }
                Err(err) => {
                    log::debug!(
                        "Authentication failed ({}) for {:?}. Result couldn't be decoded: {:?}",
                        code,
                        request,
                        err
                    );
                    Err(ClientError::Request(format!(
                        "Failed to decode service error response: {}",
                        err
                    )))
                }
            },
        }
    }
}
