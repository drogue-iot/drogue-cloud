use anyhow::Context;
use chrono::{DateTime, Utc};
use drogue_cloud_console_common::UserInfo;
use http::{Response, Uri};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{sync::RwLock, time::Duration};
use url::Url;
use yew::{format::Text, prelude::*, services::fetch::*, utils::window};

/// Backend information
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendInformation {
    pub url: Url,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub idps: Vec<IdpInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footer_band: Vec<String>,
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
    pub fn url<S: AsRef<str>>(&self, path: S) -> Url {
        let mut result = self.url.clone();
        result.set_path(path.as_ref());
        result
    }

    pub fn uri<S: AsRef<str>>(&self, path: S) -> Uri {
        self.url(path).to_string().parse().unwrap()
    }

    pub fn url_str<S: AsRef<str>>(&self, path: S) -> String {
        self.url(path).into()
    }

    pub fn request<S, IN, OUT: 'static>(
        &self,
        method: http::Method,
        path: S,
        payload: IN,
        headers: Vec<(&str, &str)>,
        callback: Callback<Response<OUT>>,
    ) -> Result<FetchTask, anyhow::Error>
    where
        S: AsRef<str>,
        IN: Into<Text>,
        OUT: From<Text>,
    {
        self.request_with(method, path, payload, headers, Default::default(), callback)
    }

    pub fn request_with<S, IN, OUT: 'static>(
        &self,
        method: http::Method,
        path: S,
        payload: IN,
        headers: Vec<(&str, &str)>,
        options: RequestOptions,
        callback: Callback<Response<OUT>>,
    ) -> Result<FetchTask, anyhow::Error>
    where
        S: AsRef<str>,
        IN: Into<Text>,
        OUT: From<Text>,
    {
        let request = http::request::Builder::new()
            .method(method)
            .uri(self.uri(path));

        let token = match Backend::access_token() {
            Some(token) => token,
            None => {
                if !options.disable_reauth {
                    Backend::reauthenticate().ok();
                    return Err(anyhow::anyhow!("Performing re-auth"));
                }
                return Err(anyhow::anyhow!("Missing token"));
            }
        };

        let mut request = request.header("Authorization", format!("Bearer {}", token));

        for (k, v) in headers {
            request = request.header(k, v);
        }

        let request = request.body(payload).context("Failed to create request")?;

        let task = FetchService::fetch_with_options(
            request,
            FetchOptions {
                cache: Some(Cache::NoCache),
                credentials: Some(Credentials::Include),
                redirect: Some(Redirect::Follow),
                mode: Some(Mode::Cors),
                ..Default::default()
            },
            callback.reform(move |response: Response<_>| {
                log::info!("Backend response code: {}", response.status().as_u16());
                match response.status().as_u16() {
                    401 | 403 | 408 if !options.disable_reauth => {
                        // 408 is "sent" by yew if the request fails, which it does when CORS is in play
                        Backend::reauthenticate().ok();
                    }
                    _ => {}
                };
                response
            }),
        )
        .map_err(|err| anyhow::anyhow!("Failed to fetch: {:?}", err))?;

        Ok(task)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RequestOptions {
    pub disable_reauth: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backend {
    pub info: BackendInformation,
    token: Option<Token>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub expires: Option<DateTime<Utc>>,
    pub id_token: String,
    pub refresh_token: Option<String>,
    pub userinfo: Option<UserInfo>,
}

impl Token {
    pub fn is_expired(&self) -> bool {
        self.valid_for()
            .map_or(false, |timeout| timeout.as_secs() < 30)
    }

    pub fn valid_for(&self) -> Option<Duration> {
        self.expires
            .map(|expires| expires.signed_duration_since(Utc::now()))
            .and_then(|expires| expires.to_std().ok())
    }

    pub fn if_valid(&self) -> Option<&Self> {
        if self.is_expired() {
            None
        } else {
            Some(self)
        }
    }
}

static CONSOLE_BACKEND: Lazy<RwLock<Option<Backend>>> = Lazy::new(|| RwLock::new(None));

impl Backend {
    /// Return the backend endpoint, or [`Option::None`].
    pub fn get() -> Option<Backend> {
        CONSOLE_BACKEND.read().unwrap().clone()
    }

    pub fn url<S: AsRef<str>>(path: S) -> Option<Url> {
        Self::get().map(|backend| backend.info.url(path))
    }

    #[allow(dead_code)]
    pub fn uri<S: AsRef<str>>(path: S) -> Option<Uri> {
        Self::get().map(|backend| backend.info.uri(path))
    }

    pub fn url_str<S: AsRef<str>>(path: S) -> Option<String> {
        Self::get().map(|backend| backend.info.url_str(path))
    }

    /// Get the access token, if it is not expired yet
    pub fn access_token() -> Option<String> {
        Self::get()
            .and_then(|b| b.token)
            .as_ref()
            .and_then(|t| t.if_valid())
            .map(|token| token.access_token.clone())
    }

    /// Get full token information
    pub fn token() -> Option<Token> {
        Self::get().and_then(|b| b.token)
    }

    pub(crate) fn set(info: Option<BackendInformation>) {
        *CONSOLE_BACKEND.write().unwrap() = info.map(|info| Backend { info, token: None });
    }

    fn update<F>(f: F)
    where
        F: FnOnce(&mut Backend),
    {
        let mut backend = CONSOLE_BACKEND.write().unwrap();
        if let Some(ref mut backend) = *backend {
            f(backend);
        }
    }

    pub(crate) fn update_token(token: Option<Token>) {
        Self::update(|backend| backend.token = token);
    }

    pub fn current_url(&self) -> String {
        self.info.url.to_string()
    }

    pub fn request<S, IN, OUT: 'static>(
        method: http::Method,
        path: S,
        payload: IN,
        callback: Callback<Response<OUT>>,
    ) -> Result<FetchTask, anyhow::Error>
    where
        S: AsRef<str>,
        IN: Into<Text>,
        OUT: From<Text>,
    {
        Self::request_with(method, path, payload, Default::default(), callback)
    }

    pub fn request_with<S, IN, OUT: 'static>(
        method: http::Method,
        path: S,
        payload: IN,
        options: RequestOptions,
        callback: Callback<Response<OUT>>,
    ) -> Result<FetchTask, anyhow::Error>
    where
        S: AsRef<str>,
        IN: Into<Text>,
        OUT: From<Text>,
    {
        Self::get()
            .ok_or_else(|| anyhow::anyhow!("Missing backend"))?
            .info
            .request_with(method, path, payload, vec![], options, callback)
    }

    pub fn reauthenticate() -> Result<(), anyhow::Error> {
        Self::navigate_to(
            "/api/console/v1alpha1/ui/login",
            "Trigger re-authenticate flow",
        )
    }

    pub fn logout() -> Result<(), anyhow::Error> {
        Self::navigate_to("/api/console/v1alpha1/ui/logout", "Trigger logout flow")
    }

    fn navigate_to<S: AsRef<str>>(path: S, op: &str) -> Result<(), anyhow::Error> {
        let target = Backend::url_str(path).context("Backend information missing");
        log::debug!("{}: {:?}", op, target);
        window().location().set_href(&target?).unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
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

    #[test]
    fn test_valid_for() {
        setup();

        let date = Utc::now() + chrono::Duration::seconds(120);

        let token = Token {
            access_token: String::new(),
            id_token: String::new(),
            refresh_token: None,
            expires: Some(date),
            userinfo: None,
        };

        assert!(!token.is_expired());
        assert!(token.valid_for().is_some());
    }
}
