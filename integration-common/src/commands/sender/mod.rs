mod default;
mod ttnv2;
mod ttnv3;

use crate::commands::CommandOptions;
use async_trait::async_trait;
use drogue_client::registry;
use drogue_cloud_service_api::webapp::web;
use reqwest::{
    header::{HeaderName, HeaderValue},
    Response, StatusCode,
};
use std::{fmt::Formatter, str::FromStr};
use thiserror::Error;
use url::Url;

pub struct Context {
    pub device_id: String,
    pub client: reqwest::Client,
}

#[async_trait]
pub trait Sender {
    async fn send(
        &self,
        ctx: Context,
        endpoint: registry::v1::ExternalEndpoint,
        command: CommandOptions,
        body: web::Bytes,
    ) -> Result<(), Error>;
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Transport failed: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("Unknown external type: {0}")]
    UnknownType(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Invalid payload: {0}")]
    Payload(String),
}

#[derive(Clone, Debug)]
pub struct HttpError(pub StatusCode, pub String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "HTTP request failed: {}\n{}", self.0, self.1)
    }
}

impl std::error::Error for HttpError {}

pub async fn send_to_external(
    ctx: Context,
    endpoint: registry::v1::ExternalEndpoint,
    command: CommandOptions,
    payload: web::Bytes,
) -> Result<(), Error> {
    match endpoint.r#type.as_deref() {
        None => {
            default::DefaultSender
                .send(ctx, endpoint, command, payload)
                .await
        }
        Some("ttn") | Some("ttnv2") => {
            ttnv2::TtnV2Sender
                .send(ctx, endpoint, command, payload)
                .await
        }
        Some("ttnv3") => {
            ttnv3::TtnV3Sender
                .send(ctx, endpoint, command, payload)
                .await
        }
        Some(t) => Err(Error::UnknownType(t.to_string())),
    }
}

/// Convert the name to an HTTP method.
///
/// If the name is empty, [`None`] is returned. If the method is invalid, and error will be returned.
fn to_method(name: &str) -> Result<Option<reqwest::Method>, Error> {
    if name.is_empty() {
        Ok(None)
    } else {
        match reqwest::Method::from_str(name) {
            Ok(m) => Ok(Some(m)),
            Err(_) => Err(Error::InvalidConfiguration(format!(
                "Invalid HTTP method: {}",
                name
            ))),
        }
    }
}

/// Takes an external endpoint and creates an HTTP request builder from it.
pub(crate) fn to_builder<F>(
    client: reqwest::Client,
    default_method: reqwest::Method,
    endpoint: &registry::v1::ExternalEndpoint,
    f: F,
) -> Result<reqwest::RequestBuilder, Error>
where
    F: FnOnce(Url) -> Result<Url, Error>,
{
    let method = to_method(&endpoint.method)?.unwrap_or(default_method);
    let url = Url::parse(&endpoint.url)
        .map_err(|err| Error::InvalidConfiguration(format!("Unable to parse URL: {}", err)))?;

    let url = f(url)?;

    let mut builder = client.request(method, url);

    for (k, v) in &endpoint.headers {
        let key = HeaderName::from_str(k).map_err(|err| {
            Error::InvalidConfiguration(format!("Invalid HTTP header key: '{}': {}", k, err))
        })?;
        let value = HeaderValue::from_str(v).map_err(|err| {
            Error::InvalidConfiguration(format!("Invalid HTTP header value: '{}': {}", v, err))
        })?;
        builder = builder.header(key, value);
    }

    Ok(builder)
}

pub(crate) async fn default_error(resp: Response) -> Error {
    let code = resp.status();
    match resp.text().await {
        Ok(text) => Error::Transport(Box::new(HttpError(code, text))),
        Err(err) => Error::Transport(Box::new(HttpError(
            code,
            format!("Failed to get error information: {}", err),
        ))),
    }
}
