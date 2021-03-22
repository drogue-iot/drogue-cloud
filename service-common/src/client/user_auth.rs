use crate::{defaults, openid::OpenIdTokenProvider};
use drogue_cloud_service_api::auth::{
    authz::{AuthorizationRequest, AuthorizationResponse},
    ClientError, ErrorInformation,
};
use reqwest::{RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

/// A client for authorizing user requests.
#[derive(Clone, Debug)]
pub struct UserAuthClient {
    client: reqwest::Client,
    auth_url: Url,
    token_provider: Option<OpenIdTokenProvider>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserAuthClientConfig {
    #[serde(default = "defaults::user_auth_url")]
    pub url: Url,
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub refresh_before: Option<Duration>,
}

impl Default for UserAuthClientConfig {
    fn default() -> Self {
        Self {
            url: defaults::user_auth_url(),
            refresh_before: None,
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
            auth_url: url.join("/api/v1/user/authz")?,
            client,
            token_provider,
        })
    }

    pub fn from_openid_client(
        config: &UserAuthClientConfig,
        client: openid::Client,
    ) -> anyhow::Result<Self> {
        Self::new(
            client.http_client.clone(),
            config.url.clone(),
            Some(OpenIdTokenProvider::new(
                client,
                config
                    .refresh_before
                    // map to chrono ...
                    .and_then(|t| chrono::Duration::from_std(t).ok())
                    // ... or fall back to 15 seconds
                    .unwrap_or_else(|| chrono::Duration::seconds(15)),
            )),
        )
    }

    async fn inject_token(
        &self,
        builder: RequestBuilder,
    ) -> Result<RequestBuilder, ClientError<reqwest::Error>> {
        if let Some(provider) = &self.token_provider {
            let token = provider
                .provide_token()
                .await
                .map_err(|err| ClientError::Token(Box::new(err)))?;
            Ok(builder.bearer_auth(token.access_token))
        } else {
            Ok(builder)
        }
    }

    pub async fn authorize(
        &self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, ClientError<reqwest::Error>> {
        let req = self.client.post(self.auth_url.clone());
        let req = self.inject_token(req).await?;

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
                    log::debug!("Authentication failed for {:?}. Result: {:?}", request, err);

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
                        "Service call failed ({}) for {:?}. Result couldn't be decoded: {:?}",
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
