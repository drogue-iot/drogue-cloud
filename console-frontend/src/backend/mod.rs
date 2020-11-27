use anyhow::Context;
use http::{Response, Uri};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
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
    token: Option<String>,
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

    pub fn token() -> Option<String> {
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

    pub(crate) fn update_token(token: Option<String>) {
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

        let token = match Backend::token() {
            Some(token) => token,
            None => {
                Self::reauth();
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
                if response.status().as_u16() == 401 {
                    // handle auth-error
                    Self::reauth();
                }
                response
            }),
        )
        .map_err(|err| anyhow::anyhow!("Failed to fetch: {:?}", err))?;

        Ok(task)
    }

    fn reauth() {
        // need to authenticate
        let location = window().location();
        location
            .set_href(&Backend::url_str("/ui/login").unwrap())
            .unwrap();
    }
}
