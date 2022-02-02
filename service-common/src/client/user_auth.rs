use crate::{defaults, openid::TokenConfig};
use drogue_client::{
    core::WithTracing,
    error::ClientError,
    openid::{OpenIdTokenProvider, TokenInjector},
};
use drogue_cloud_service_api::auth::user::{
    authn::{AuthenticationRequest, AuthenticationResponse},
    authz::{AuthorizationRequest, AuthorizationResponse},
};
use reqwest::{Response, StatusCode};
use serde::Deserialize;
use tracing::instrument;
use url::Url;

/// A client for authorizing user requests.
#[derive(Clone, Debug)]
pub struct UserAuthClient {
    client: reqwest::Client,
    authn_url: Url,
    authz_url: Url,
    token_provider: Option<OpenIdTokenProvider>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UserAuthClientConfig {
    #[serde(default = "defaults::user_auth_url")]
    pub url: Url,

    #[serde(flatten, default)]
    pub token_config: Option<TokenConfig>,
}

impl UserAuthClient {
    /// Create a new client instance.
    pub fn new(
        client: reqwest::Client,
        url: Url,
        token_provider: Option<OpenIdTokenProvider>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            authn_url: url.join("/api/user/v1alpha1/authn")?,
            authz_url: url.join("/api/v1/user/authz")?,
            client,
            token_provider,
        })
    }

    pub async fn from_config(
        client: reqwest::Client,
        config: UserAuthClientConfig,
    ) -> anyhow::Result<Self> {
        let token_provider = if let Some(config) = config.token_config {
            Some(config.discover_from(client.clone()).await?)
        } else {
            None
        };
        Self::new(client, config.url, token_provider)
    }

    #[instrument]
    pub async fn authenticate_access_token(
        &self,
        request: AuthenticationRequest,
    ) -> Result<AuthenticationResponse, ClientError<reqwest::Error>> {
        let req = self
            .client
            .post(self.authn_url.clone())
            .propagate_current_context()
            .inject_token(&self.token_provider)
            .await?;

        let response: Response = req.json(&request).send().await.map_err(|err| {
            log::warn!("Error while authenticating {:?}: {}", request, err);
            Box::new(err)
        })?;

        match response.status() {
            StatusCode::OK => match response.json::<AuthenticationResponse>().await {
                Ok(result) => {
                    log::debug!("Outcome for {:?} is {:?}", request, result);
                    Ok(result)
                }
                Err(err) => {
                    log::debug!("Authentication failed for {:?}. Result: {:?}", request, err);

                    Err(ClientError::Request(format!(
                        "Failed to decode service response: {}",
                        err
                    )))
                }
            },
            code => super::default_error(code, response).await,
        }
    }

    #[instrument]
    pub async fn authorize(
        &self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, ClientError<reqwest::Error>> {
        let req = self
            .client
            .post(self.authz_url.clone())
            .propagate_current_context()
            .inject_token(&self.token_provider)
            .await?;

        let response: Response = req.json(&request).send().await.map_err(|err| {
            log::warn!("Error while authorizing {:?}: {}", request, err);
            Box::new(err)
        })?;

        match response.status() {
            StatusCode::OK => match response.json::<AuthorizationResponse>().await {
                Ok(result) => {
                    log::debug!("Outcome for {:?} is {:?}", request, result);
                    Ok(result)
                }
                Err(err) => {
                    log::debug!("Authorization failed for {:?}. Result: {:?}", request, err);

                    Err(ClientError::Request(format!(
                        "Failed to decode service response: {}",
                        err
                    )))
                }
            },
            code => super::default_error(code, response).await,
        }
    }
}
