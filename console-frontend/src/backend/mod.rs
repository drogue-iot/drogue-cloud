use anyhow::Context;
use chrono::{DateTime, Utc};
use drogue_cloud_console_common::UserInfo;
use http::{Response, Uri};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::time::Duration;
use url::Url;
use yew::{format::Text, prelude::*, services::fetch::*, utils::window};

/// Backend information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackendInformation {
    pub url: Url,
}

#[derive(Clone, Debug)]
pub struct Backend {
    info: BackendInformation,
    token: Option<Token>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub id_token: String,
    pub refresh_token: Option<String>,
    pub expires: Option<DateTime<Utc>>,
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
        Self::get().map(|backend| {
            let mut result = backend.info.url;
            result.set_path(path.as_ref());
            result
        })
    }

    pub fn uri<S: AsRef<str>>(path: S) -> Option<Uri> {
        Self::url(path).map(|url| url.to_string().parse().unwrap())
    }

    pub fn url_str<S: AsRef<str>>(path: S) -> Option<String> {
        Self::url(path).map(|url| url.to_string())
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
        let request = http::request::Builder::new().method(method);

        let request =
            request.uri(Self::uri(path).ok_or_else(|| anyhow::anyhow!("Missing backend"))?);

        let token = match Backend::access_token() {
            Some(token) => token,
            None => {
                Self::reauthenticate().ok();
                return Err(anyhow::anyhow!("Performing re-auth"));
            }
        };

        let request = request.header("Authorization", format!("Bearer {}", token));
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
            callback.reform(|response: Response<_>| {
                log::info!("Backend response code: {}", response.status().as_u16());
                match response.status().as_u16() {
                    401 | 403 => {
                        Self::reauthenticate().ok();
                    }
                    _ => {}
                };
                response
            }),
        )
        .map_err(|err| anyhow::anyhow!("Failed to fetch: {:?}", err))?;

        Ok(task)
    }

    pub fn reauthenticate() -> Result<(), anyhow::Error> {
        log::info!("Triggering re-authentication flow");
        // need to authenticate
        let location = window().location();
        location
            .set_href(&Backend::url_str("/ui/login").context("Backend information missing")?)
            .unwrap();
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
