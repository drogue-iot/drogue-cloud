use crate::{
    backend::{Backend, BackendInformation, Token},
    components::placeholder::Placeholder,
    data::{SharedDataBridge, SharedDataOps},
    error::error,
    page::AppPage,
    preferences::Preferences,
};
use anyhow::Error;
use chrono::{DateTime, Utc};
use drogue_cloud_console_common::UserInfo;
use patternfly_yew::*;
use std::time::Duration;
use url::Url;
use wasm_bindgen::JsValue;
use yew::{
    format::{Json, Nothing},
    prelude::*,
    services::{
        fetch::{Request, *},
        timeout::*,
    },
    utils::window,
};

pub struct Main {
    link: ComponentLink<Self>,
    access_code: Option<String>,
    task: Option<FetchTask>,
    refresh_task: Option<TimeoutTask>,
    token_holder: SharedDataBridge<Option<Token>>,
    /// Something failed, we can no longer work.
    app_failure: bool,
    /// We are in the process of authenticating.
    authenticating: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Trigger fetching the endpoint information
    FetchEndpoint,
    /// Failed to fetch endpoint information
    FetchBackendFailed,
    /// Trigger an overall application failure
    AppFailure(Toast),
    /// Set the backend information
    Endpoint(BackendInformation),
    /// Exchange the authentication code for an access token
    GetToken(String),
    /// Set the access token
    SetAccessToken(Option<Token>),
    /// Callback when fetching the token failed
    FetchTokenFailed,
    RetryLogin,
    /// Send to trigger refreshing the access token
    RefreshToken(Option<String>),
    /// Trigger logout
    Logout,
}

impl Component for Main {
    type Message = Msg;
    type Properties = ();
    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchEndpoint);

        let location = window().location();
        let url = Url::parse(&location.href().unwrap()).unwrap();

        log::debug!("href: {:?}", url);

        let code = url.query_pairs().find_map(|(k, v)| {
            if k == "code" {
                Some(v.to_string())
            } else {
                None
            }
        });

        let error = url.query_pairs().find_map(|(k, v)| {
            if k == "error" {
                Some(v.to_string())
            } else {
                None
            }
        });

        log::debug!("Access code: {:?}", code);
        log::debug!("Login error: {:?}", error);

        if let Some(error) = error {
            link.send_message(Msg::AppFailure(Toast {
                title: "Failed to log in".into(),
                body: html! {<p>{error}</p>},
                r#type: Type::Danger,
                actions: vec![link.callback(|_| Msg::RetryLogin).into_action("Retry")],
                ..Default::default()
            }));
        }

        // remove code, state and others from the URL bar
        {
            let mut url = url;
            url.query_pairs_mut().clear();
            let url = url.as_str().trim_end_matches('?');
            window()
                .history()
                .unwrap()
                .replace_state_with_url(&JsValue::NULL, "Drogue IoT", Some(url))
                .ok();
        }

        let token_holder = SharedDataBridge::from(&link, Msg::SetAccessToken);

        Self {
            link,
            access_code: code,
            task: None,
            refresh_task: None,
            app_failure: false,
            authenticating: false,
            token_holder,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::FetchEndpoint => {
                self.task = Some(
                    self.fetch_backend()
                        .expect("Failed to get backend information"),
                );
                true
            }
            Msg::Endpoint(backend) => {
                log::info!("Got backend: {:?}", backend);
                Backend::set(Some(backend));
                self.task = None;
                if !self.app_failure {
                    if let Some(access_code) = self.access_code.take() {
                        // exchange code for token if we have a code and no app failure
                        log::info!("Exchange access code for token");
                        self.authenticating = true;
                        self.link.send_message(Msg::GetToken(access_code));
                    } else if let Some(refresh) = Preferences::load()
                        .ok()
                        .and_then(|prefs| prefs.refresh_token)
                    {
                        log::info!("Re-using existing refresh token");
                        self.authenticating = true;
                        // try using existing refresh token
                        self.link.send_message(Msg::RefreshToken(Some(refresh)))
                    }
                }

                true
            }
            Msg::FetchBackendFailed => {
                error(
                    "Failed to fetch backend information",
                    "Could not retrieve information for connecting to the backend.",
                );
                true
            }
            Msg::AppFailure(toast) => {
                ToastDispatcher::default().toast(toast);
                self.app_failure = true;
                true
            }
            Msg::FetchTokenFailed => {
                self.authenticating = false;
                true
            }
            Msg::RetryLogin => {
                Backend::update_token(None);
                if Backend::reauthenticate().is_err() {
                    error(
                        "Failed to log in",
                        "No backed information present. Unable to trigger login.",
                    );
                }
                false
            }
            Msg::GetToken(access_code) => {
                // get the access token from the code
                // this can only be called once the backend information
                if Backend::get().is_some() {
                    self.task = Some(
                        self.fetch_token(&access_code)
                            .expect("Failed to create request"),
                    );
                } else {
                    self.access_code = Some(access_code);
                }
                true
            }
            Msg::SetAccessToken(Some(token)) => {
                log::info!("Token: {:?}", token);
                self.task = None;
                self.authenticating = false;
                Preferences::update_or_default(|mut prefs| {
                    prefs.refresh_token = token.refresh_token.as_ref().cloned();
                    prefs.id_token = token.id_token.clone();
                    prefs.user_info = token.userinfo.as_ref().cloned();
                    Ok(prefs)
                })
                .map_err(|err| {
                    log::warn!("Failed to store preferences: {}", err);
                    err
                })
                .ok();

                Backend::update_token(Some(token.clone()));
                if let Some(timeout) = token.valid_for() {
                    log::info!("Token expires in {:?}", timeout);

                    let mut rem = (timeout.as_secs() as i64) - 30;
                    if rem < 0 {
                        // ensure we are non-negative
                        rem = 0;
                    }

                    if rem < 30 {
                        // refresh now
                        log::debug!("Scheduling refresh now (had {} s remaining)", rem);
                        self.link
                            .send_message(Msg::RefreshToken(token.refresh_token.as_ref().cloned()));
                    } else {
                        log::debug!("Scheduling refresh in {} seconds", rem);
                        let refresh_token = token.refresh_token.as_ref().cloned();
                        self.refresh_task = Some(TimeoutService::spawn(
                            Duration::from_secs(rem as u64),
                            self.link.callback_once(move |_| {
                                log::info!("Token timer expired, refreshing...");
                                Msg::RefreshToken(refresh_token)
                            }),
                        ));
                    }
                } else {
                    log::debug!("Token has no expiration set");
                }

                // announce the new token

                self.token_holder.set(Some(token));

                // done

                true
            }
            Msg::SetAccessToken(None) => true,
            Msg::RefreshToken(refresh_token) => {
                log::info!("Refreshing access token");

                match refresh_token {
                    Some(refresh_token) => {
                        self.task = match self.refresh_token(&refresh_token) {
                            Ok(task) => Some(task),
                            Err(_) => {
                                Backend::reauthenticate().ok();
                                None
                            }
                        }
                    }
                    None => {
                        Backend::reauthenticate().ok();
                    }
                }

                true
            }
            Msg::Logout => {
                Preferences::update_or_default(|mut prefs| {
                    prefs.refresh_token = None;
                    prefs.id_token = Default::default();
                    prefs.user_info = None;
                    Ok(prefs)
                })
                .ok();
                Backend::logout().ok();
                false
            }
        }
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        return html! {
            <>
                <BackdropViewer/>
                <ToastViewer/>

                {
                    if let Some(ready) = self.is_ready() {

                        html!{
                            <AppPage
                                backend=ready.0
                                token=ready.1
                                on_logout=self.link.callback(|_|Msg::Logout)
                                />
                        }

                    } else if self.need_login() {
                        html!{ <Placeholder/> }
                    } else {
                        html!{}
                    }
                }

            </>
        };
    }
}

impl Main {
    /// Check if the app and backend are ready to show the application.
    fn is_ready(&self) -> Option<(Backend, Token)> {
        match (self.app_failure, Backend::get(), Backend::token()) {
            (true, _, _) => None,
            (false, Some(backend), Some(token)) => Some((backend, token)),
            _ => None,
        }
    }

    fn need_login(&self) -> bool {
        !self.app_failure && Backend::get().is_some() && !self.authenticating
    }

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
                    Msg::FetchBackendFailed
                },
            ),
        )
    }

    fn refresh_token(&self, refresh_token: &str) -> Result<FetchTask, anyhow::Error> {
        let mut url = Backend::url("/api/console/v1alpha1/ui/refresh")
            .ok_or_else(|| anyhow::anyhow!("Missing backend information"))?;

        url.query_pairs_mut()
            .append_pair("refresh_token", refresh_token);

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
                    log::info!("Response from refreshing token: {:?}", response);
                    Self::from_response(response, true)
                },
            ),
        )
    }

    fn fetch_token<S: AsRef<str>>(&self, access_code: S) -> Result<FetchTask, anyhow::Error> {
        let mut url = Backend::url("/api/console/v1alpha1/ui/token")
            .ok_or_else(|| anyhow::anyhow!("Missing backend information"))?;

        url.query_pairs_mut()
            .append_pair("code", access_code.as_ref());

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
                    log::info!("Code to token response: {:?}", response);
                    Self::from_response(response, false)
                },
            ),
        )
    }

    fn from_response(
        response: Response<Json<Result<serde_json::Value, Error>>>,
        is_refresh: bool,
    ) -> Msg {
        if let (meta, Json(Ok(value))) = response.into_parts() {
            if meta.status.is_success() {
                let access_token = value["bearer"]["access_token"]
                    .as_str()
                    .map(|s| s.to_string());
                let refresh_token = value["bearer"]["refresh_token"]
                    .as_str()
                    .map(|s| s.to_string());
                let id_token = value["bearer"]["id_token"].as_str().map(|s| s.to_string());

                let userinfo: Option<UserInfo> =
                    serde_json::from_value(value["userinfo"].clone()).unwrap_or_default();

                let expires = match value["expires"].as_str() {
                    Some(expires) => DateTime::parse_from_rfc3339(expires).ok(),
                    None => None,
                }
                .map(|expires| expires.with_timezone(&Utc));

                let token = match (access_token, id_token) {
                    (Some(access_token), Some(id_token)) => {
                        if !is_refresh {
                            Some(Token {
                                access_token,
                                refresh_token,
                                id_token,
                                expires,
                                userinfo,
                            })
                        } else if let Ok(prefs) = Preferences::load() {
                            Some(Token {
                                access_token,
                                refresh_token,
                                id_token: prefs.id_token,
                                expires,
                                userinfo: prefs.user_info,
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                log::info!("Token: {:?}", token);

                match token {
                    Some(token) => Msg::SetAccessToken(Some(token)),
                    None => Msg::FetchTokenFailed,
                }
            } else {
                Msg::FetchTokenFailed
            }
        } else {
            Msg::FetchTokenFailed
        }
    }
}
