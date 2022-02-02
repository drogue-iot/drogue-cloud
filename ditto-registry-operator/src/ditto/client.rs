use crate::ditto::{
    api::{self, RequestFactory},
    devops::DevopsCommand,
};
use drogue_client::{
    error::ClientError,
    openid::{TokenInjector, TokenProvider},
};
use http::{Method, StatusCode};
use log::log_enabled;
use reqwest::RequestBuilder;
use serde::Deserialize;
use serde_json::Value;
use std::{fmt::Debug, time::Duration};
use url::{ParseError, Url};

#[derive(Debug, Clone)]
pub struct Client {
    client: reqwest::Client,
    url: Url,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to build URL: {0}")]
    Url(#[from] ParseError),
    #[error("failed to acquire access token: {0}")]
    Token(ClientError<reqwest::Error>),
    #[error("failed to execute request: {0}")]
    Request(#[from] reqwest::Error),
    #[error("response indicated error ({0})")]
    Response(StatusCode),
    #[error("JSON error ({0})")]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Check if the error is temporary, so that we can re-try
    pub fn is_temporary(&self) -> bool {
        match self {
            Self::Request(err) => {
                err.is_timeout()
                    || err.is_connect()
                    || err
                        .status()
                        .map(|code| Self::code_is_temp(&code))
                        .unwrap_or_default()
            }
            Self::Response(code) => Self::code_is_temp(code),
            _ => false,
        }
    }

    /// Check if the status code indicates a temporary error.
    ///
    /// Currently a server error and a request timeout (408) are considered temporary.
    fn code_is_temp(code: &StatusCode) -> bool {
        code.is_server_error() || code.as_u16() == 408
    }
}

impl Client {
    pub fn new(client: reqwest::Client, url: Url) -> Self {
        Self { client, url }
    }

    pub async fn request<TP, R>(
        &self,
        token_provider: &TP,
        request: R,
    ) -> Result<Option<R::Response>, Error>
    where
        TP: TokenProvider,
        R: api::Request,
        R::Response: for<'de> Deserialize<'de>,
    {
        let req = request
            .into_builder(self)?
            .inject_token(token_provider)
            .await
            .map_err(Error::Token)?;

        let resp = req.send().await?;

        if resp.status().is_success() {
            match resp.status().as_u16() {
                204 => Ok(None),
                _ => Ok(Some(resp.json().await?)),
            }
        } else {
            let status = resp.status();
            if log_enabled!(log::Level::Debug) {
                let payload = resp.text().await?;
                log::debug!("Request failed ({}): {}", payload, status);
            }
            Err(Error::Response(status))
        }
    }

    pub async fn devops<TP, R>(
        &self,
        token_provider: &TP,
        timeout: Option<Duration>,
        command: &DevopsCommand,
    ) -> Result<R, Error>
    where
        TP: TokenProvider,
        R: for<'de> Deserialize<'de>,
    {
        let mut url = self.url.join("/devops/piggyback/connectivity")?;

        if let Some(timeout) = timeout {
            url.query_pairs_mut()
                .append_pair("timeout", &format!("{}s", timeout.as_secs()));
        }

        let req = self
            .client
            .post(url)
            .inject_token(token_provider)
            .await
            .map_err(Error::Token)?;

        let req = req.json(command);

        log::debug!("Ditto request: {:#?}", req);
        if log::log_enabled!(log::Level::Debug) {
            let json = serde_json::to_string_pretty(command).unwrap_or_default();
            log::debug!("Payload: {}", json);
        }

        let resp = req.send().await?.error_for_status()?;
        let resp: Value = resp.json().await?;

        if log::log_enabled!(log::Level::Debug) {
            log::debug!("Response: {:#?}", resp);
        }

        Ok(serde_json::from_value(resp)?)
    }
}

impl RequestFactory for Client {
    fn new_request<S: AsRef<str>>(&self, method: Method, path: S) -> Result<RequestBuilder, Error> {
        let url = self.url.join("/api/2/")?.join(path.as_ref())?;
        Ok(self.client.request(method, url))
    }
}
