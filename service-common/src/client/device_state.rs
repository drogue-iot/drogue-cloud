use crate::{defaults, openid::TokenConfig, reqwest::ClientFactory, tls::ClientConfig};
use drogue_client::{
    core::PropagateCurrentContext,
    error::ClientError,
    openid::{OpenIdTokenProvider, TokenInjector},
};
use drogue_cloud_service_api::services::device_state::{
    CreateRequest, CreateResponse, DeleteOptions, DeleteRequest, DeviceState, InitResponse,
    PingResponse,
};
use k8s_openapi::percent_encoding::{percent_encode, NON_ALPHANUMERIC};
use reqwest::{Response, StatusCode};
use serde::Deserialize;
use std::fmt::Debug;
use tracing::instrument;
use url::Url;

/// A client for authorizing user requests.
#[derive(Clone, Debug)]
pub struct DeviceStateClient {
    client: reqwest::Client,
    url: Url,
    token_provider: Option<OpenIdTokenProvider>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceStateClientConfig {
    #[serde(default = "defaults::device_state_url")]
    pub url: Url,

    #[serde(default)]
    pub client: ClientConfig,

    #[serde(flatten, default)]
    pub token_config: Option<TokenConfig>,
}

impl Default for DeviceStateClientConfig {
    fn default() -> Self {
        Self {
            url: defaults::device_state_url(),
            client: Default::default(),
            token_config: None,
        }
    }
}

impl DeviceStateClient {
    /// Create a new client instance.
    pub fn new(
        client: reqwest::Client,
        url: Url,
        token_provider: Option<OpenIdTokenProvider>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            url,
            client,
            token_provider,
        })
    }

    pub async fn from_config(config: DeviceStateClientConfig) -> anyhow::Result<Self> {
        let token_provider = if let Some(config) = config.token_config {
            Some(config.discover_from().await?)
        } else {
            None
        };

        Self::new(
            ClientFactory::from(config.client).build()?,
            config.url,
            token_provider,
        )
    }

    #[instrument(err)]
    pub async fn init(&self) -> Result<InitResponse, ClientError> {
        let url = self.url.join("/api/state/v1alpha1/sessions")?;

        let req = self
            .client
            .put(url)
            .propagate_current_context()
            .inject_token(&self.token_provider)
            .await?;

        let response: Response = req
            .send()
            .await
            .map_err(|err| ClientError::Client(Box::new(err)))?;

        handle_response(response, StatusCode::CREATED).await
    }

    #[instrument(level = "debug", err)]
    pub async fn ping(&self, session: &str) -> Result<PingResponse, ClientError> {
        let url = self.url.join(&format!(
            "/api/state/v1alpha1/sessions/{}",
            percent_encode(session.as_bytes(), NON_ALPHANUMERIC)
        ))?;

        let req = self
            .client
            .post(url)
            .propagate_current_context()
            .inject_token(&self.token_provider)
            .await?;

        let response: Response = req
            .send()
            .await
            .map_err(|err| ClientError::Client(Box::new(err)))?;

        handle_response(response, StatusCode::OK).await
    }

    #[instrument(err)]
    pub async fn create(
        &self,
        session: &str,
        application: &str,
        device: &str,
        token: &str,
        state: DeviceState,
    ) -> Result<CreateResponse, ClientError> {
        let url = self.state_url(session, application, device)?;

        let req = self
            .client
            .put(url)
            .propagate_current_context()
            .inject_token(&self.token_provider)
            .await?
            .json(&CreateRequest {
                token: token.to_string(),
                state,
            });

        let response: Response = req
            .send()
            .await
            .map_err(|err| ClientError::Client(Box::new(err)))?;

        match response.status() {
            StatusCode::CREATED => Ok(CreateResponse::Created),
            StatusCode::CONFLICT => Ok(CreateResponse::Occupied),
            code => super::default_error(code, response).await,
        }
    }

    #[instrument(err)]
    pub async fn delete(
        &self,
        session: &str,
        application: &str,
        device: &str,
        token: &str,
        opts: &DeleteOptions,
    ) -> Result<(), ClientError> {
        let url = self.state_url(session, application, device)?;

        let req = self
            .client
            .delete(url)
            .propagate_current_context()
            .inject_token(&self.token_provider)
            .await?
            .json(&DeleteRequest {
                token: token.to_string(),
                options: opts.clone(),
            });

        let response: Response = req
            .send()
            .await
            .map_err(|err| ClientError::Client(Box::new(err)))?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(()),
            code => super::default_error(code, response).await,
        }
    }

    fn state_url(
        &self,
        session: &str,
        application: &str,
        device: &str,
    ) -> Result<Url, ClientError> {
        Ok(self.url.join(&format!(
            "/api/state/v1alpha1/sessions/{}/states/{}/{}",
            percent_encode(session.as_bytes(), NON_ALPHANUMERIC),
            percent_encode(application.as_bytes(), NON_ALPHANUMERIC),
            percent_encode(device.as_bytes(), NON_ALPHANUMERIC)
        ))?)
    }
}

async fn handle_response<T>(response: Response, expected_code: StatusCode) -> Result<T, ClientError>
where
    T: for<'de> Deserialize<'de> + Debug,
{
    let code = response.status();
    if code == expected_code {
        match response.json::<T>().await {
            Ok(result) => {
                log::debug!("Outcome is {result:?}");
                Ok(result)
            }
            Err(err) => {
                log::debug!("Request failed. Result: {err:?}");

                Err(ClientError::Request(format!(
                    "Failed to decode service response: {err}"
                )))
            }
        }
    } else {
        super::default_error(code, response).await
    }
}
