#![recursion_limit = "512"]

mod components;
mod index;
mod placeholder;
mod spy;

use anyhow::{Context, Error};
use std::sync::RwLock;

use wasm_bindgen::prelude::*;

use patternfly_yew::*;
use yew::{
    format::{Json, Nothing},
    prelude::*,
    services::fetch::*,
};
use yew_router::prelude::*;

use serde::{Deserialize, Serialize};

use once_cell::sync::Lazy;

use crate::index::Index;
use crate::placeholder::Placeholder;
use crate::spy::Spy;
use url::Url;
use yew::format::Text;
use yew::services::storage::*;
use yew::utils::window;

#[derive(Switch, Debug, Clone, PartialEq)]
pub enum AppRoute {
    #[to = "/spy"]
    Spy,
    #[to = "/"]
    Index,
}

struct Main {
    link: ComponentLink<Self>,
    storage: StorageService,
    task: Option<FetchTask>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    FetchEndpoint,
    FetchFailed,
    Endpoint(BackendInformation),
    UpdateToken(Option<String>),
}

impl Component for Main {
    type Message = Msg;
    type Properties = ();
    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchEndpoint);

        let mut storage =
            StorageService::new(Area::Session).expect("storage was disabled by the user");

        let location = window().location();
        let url = Url::parse(&location.href().unwrap()).unwrap();

        log::info!("href: {:?}", url);

        let code = url
            .query_pairs()
            .into_iter()
            .find_map(|(k, v)| if k == "code" { Some(v) } else { None });

        if let Some(code) = code {
            let code = code.to_string();
            log::info!("Code: {}", code);
            storage.store("code", Ok(code.clone()));
        }

        Self {
            link,
            storage,
            task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::FetchEndpoint => {
                self.task = Some(self.fetch_backend().expect("Failed to create request"));
                true
            }
            Msg::Endpoint(backend) => {
                Backend::set(Some(backend));
                self.task = None;
                let token = self.storage.restore("token");
                log::info!("Checking token: {:?}", token);
                if let Ok(token) = token {
                    // re-use existing token
                    self.link.send_message(Msg::UpdateToken(Some(token)));
                } else {
                    let code = self.storage.restore("code");
                    log::info!("Checking code: {:?}", code);
                    if let Ok(code) = code {
                        self.task =
                            Some(self.fetch_token(&code).expect("Failed to create request"));
                    }
                }
                true
            }
            Msg::FetchFailed => false,
            Msg::UpdateToken(token) => {
                log::info!("Token: {:?}", token);
                Backend::update_token(token.clone());
                if let Some(token) = token {
                    self.storage.store("token", Ok(token));
                }
                self.task = None;
                true
            }
        }
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        let sidebar = match Backend::get().is_some() {
            true => html_nested! {
                <PageSidebar>
                    <Nav>
                        <NavList>
                            <NavRouterItem<AppRoute> to=AppRoute::Index>{"Home"}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Spy>{"Spy"}</NavRouterItem<AppRoute>>
                        </NavList>
                    </Nav>
                </PageSidebar>
            },
            false => html_nested! {
                <PageSidebar>
                </PageSidebar>
            },
        };

        html! {
            <Page
                logo={html_nested!{
                    <Logo src="/images/logo.png" alt="Drogue IoT" />
                }}
                sidebar=sidebar
                >
                {
                    if self.task.is_none() {
                        html!{
                            <Router<AppRoute, ()>
                                    redirect = Router::redirect(|_|AppRoute::Index)
                                    render = Router::render(|switch: AppRoute| {
                                        match switch {
                                            AppRoute::Spy => html!{<Spy/>},
                                            AppRoute::Index => html!{<Index/>},
                                        }
                                    })
                                />
                        }
                    } else {
                        html!{
                            <Placeholder/>
                        }
                    }
                }
            </Page>
        }
    }
}

impl Main {
    fn fetch_backend(&self) -> Result<FetchTask, anyhow::Error> {
        let req = Request::get("/endpoints/backend.json").body(Nothing)?;

        let opts = FetchOptions {
            cache: Some(Cache::NoCache),
            ..Default::default()
        };

        FetchService::fetch_with_options(
            req,
            opts,
            self.link.callback(
                |response: Response<Json<Result<BackendInformation, Error>>>| {
                    log::info!("Backend: {:?}", response);
                    if let (meta, Json(Ok(body))) = response.into_parts() {
                        if meta.status.is_success() {
                            return Msg::Endpoint(body);
                        }
                    }
                    Msg::FetchFailed
                },
            ),
        )
    }

    fn fetch_token(&self, code: &str) -> Result<FetchTask, anyhow::Error> {
        let mut url = Backend::url("/ui/token")
            .ok_or_else(|| anyhow::anyhow!("Missing backend information"))?;

        url.query_pairs_mut().append_pair("code", code);

        let req = Request::get(url.to_string()).body(Nothing)?;

        let opts = FetchOptions {
            cache: Some(Cache::NoCache),
            ..Default::default()
        };

        FetchService::fetch_with_options(
            req,
            opts,
            self.link.callback(
                |response: Response<Json<Result<serde_json::Value, Error>>>| {
                    log::info!("Token: {:?}", response);
                    if let (meta, Json(Ok(value))) = response.into_parts() {
                        if meta.status.is_success() {
                            return Msg::UpdateToken(
                                value["bearer"]["access_token"]
                                    .as_str()
                                    .map(|s| s.to_string()),
                            );
                        }
                    }
                    Msg::UpdateToken(None)
                },
            ),
        )
    }
}

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
            let mut result = backend.info.url.clone();
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

#[wasm_bindgen]
pub fn run_app() -> Result<(), JsValue> {
    wasm_logger::init(wasm_logger::Config::default());
    log::info!("Getting ready...");
    yew::start_app::<Main>();
    Ok(())
}
