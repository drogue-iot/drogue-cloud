use crate::{config::ConfigFromEnv, defaults, openid::TokenConfig};
use drogue_client::{
    error::ClientError,
    openid::{OpenIdTokenProvider, TokenInjector},
    Context,
};
use drogue_cloud_service_api::auth::user::{
    authn::{AuthenticationRequest, AuthenticationResponse},
    authz::{AuthorizationRequest, AuthorizationResponse},
};
use reqwest::{Response, StatusCode};
use serde::Deserialize;
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

    #[serde(default)]
    pub token_config: Option<TokenConfig>,
}

impl Default for UserAuthClientConfig {
    fn default() -> Self {
        Self {
            url: defaults::user_auth_url(),
            token_config: TokenConfig::from_env_prefix("USER_AUTH")
                .map(|v| v.amend_with_env())
                .ok(),
        }
    }
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

    pub async fn authenticate_api_key(
        &self,
        request: AuthenticationRequest,
        context: Context,
    ) -> Result<AuthenticationResponse, ClientError<reqwest::Error>> {
        let req = self
            .client
            .post(self.authn_url.clone())
            .inject_token(&self.token_provider, context)
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

    pub async fn authorize(
        &self,
        request: AuthorizationRequest,
        context: Context,
    ) -> Result<AuthorizationResponse, ClientError<reqwest::Error>> {
        let req = self
            .client
            .post(self.authz_url.clone())
            .inject_token(&self.token_provider, context)
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
