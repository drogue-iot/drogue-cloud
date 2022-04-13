mod request;

pub use request::*;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use web_sys::{RequestCache, RequestMode, RequestRedirect};
use yew_oauth2::prelude::*;

#[derive(Debug, Error)]
pub enum RequestError<E> {
    #[error("Payload conversion error: {0}")]
    PayloadConversion(E),
}

/// The handle to our backend.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendInformation {
    /// The API URL.
    pub url: Url,

    pub openid: yew_oauth2::openid::Config,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub idps: Vec<IdpInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footer_band: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuthenticatedBackend {
    backend: BackendInformation,
    authentication: Authentication,
}

impl AuthenticatedBackend {
    pub fn new(backend: BackendInformation, authentication: Authentication) -> Self {
        Self {
            backend,
            authentication,
        }
    }
}

impl Deref for AuthenticatedBackend {
    type Target = BackendInformation;

    fn deref(&self) -> &Self::Target {
        &self.backend
    }
}

impl DerefMut for AuthenticatedBackend {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.backend
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdpInfo {
    pub id: String,
    pub icon_html: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl BackendInformation {
    pub fn authenticated(&self, auth: Authentication) -> AuthenticatedBackend {
        AuthenticatedBackend::new(self.clone(), auth)
    }

    pub fn url<S: AsRef<str>>(&self, path: S) -> Url {
        let mut result = self.url.clone();
        result.set_path(path.as_ref());
        result
    }

    pub fn unauth_request_with<S, IN, H>(
        &self,
        method: http::Method,
        path: S,
        payload: IN,
        headers: Vec<(String, String)>,
        handler: H,
    ) -> Result<RequestHandle, RequestError<IN::Error>>
    where
        S: AsRef<str>,
        IN: RequestPayload,
        H: RequestHandler<anyhow::Result<Response>>,
    {
        let mut request = RequestBuilder::new(method, self.url(path));

        for (k, v) in headers {
            request = request.header(k.into(), v.into());
        }

        request = request
            .body(payload)
            .map_err(RequestError::PayloadConversion)?;

        request = request
            .cache(RequestCache::NoCache)
            .redirect(RequestRedirect::Follow)
            .mode(RequestMode::Cors);

        Ok(request.send(handler))
    }
}

impl AuthenticatedBackend {
    pub fn request<S, IN, H>(
        &self,
        method: http::Method,
        path: S,
        payload: IN,
        headers: Vec<(String, String)>,
        handler: H,
    ) -> Result<RequestHandle, RequestError<IN::Error>>
    where
        S: AsRef<str>,
        IN: RequestPayload,
        H: RequestHandler<anyhow::Result<Response>>,
    {
        self.request_with(method, path, payload, headers, handler)
    }

    pub fn request_with<S, IN, H>(
        &self,
        method: http::Method,
        path: S,
        payload: IN,
        mut headers: Vec<(String, String)>,
        handler: H,
    ) -> Result<RequestHandle, RequestError<IN::Error>>
    where
        S: AsRef<str>,
        IN: RequestPayload,
        H: RequestHandler<anyhow::Result<Response>>,
    {
        let bearer = format!("Bearer {}", self.authentication.access_token);
        headers.push(("Authorization".into(), bearer));

        self.backend
            .unauth_request_with(method, path, payload, headers, handler)
    }
}

#[cfg(test)]
mod test {

    use chrono::DateTime;

    fn setup() {
        /*
        env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .init();
         */
    }

    #[test]
    fn test_date_parser() {
        setup();

        let str = "2020-11-30T11:33:37.437915952Z";
        let date = DateTime::parse_from_rfc3339(str);
        assert!(date.is_ok());
    }
}
