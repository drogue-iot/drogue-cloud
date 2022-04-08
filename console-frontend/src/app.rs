use crate::backend::{
    ApiResponse, Json, JsonHandlerScopeExt, JsonResponse, Nothing, RequestBuilder, RequestHandle,
};
use crate::{
    backend::{Backend, BackendInformation, RequestOptions, Token},
    components::placeholder::Placeholder,
    console::Console,
    data::{SharedDataBridge, SharedDataOps},
    error::error,
    preferences::Preferences,
};
use chrono::{DateTime, Utc};
use drogue_cloud_console_common::{EndpointInformation, UserInfo};
use gloo_timers::callback::Timeout;
use gloo_utils::window;
use http::Method;
use patternfly_yew::*;
use serde_json::Value;
use std::{rc::Rc, time::Duration};
use url::Url;
use wasm_bindgen::JsValue;
use web_sys::RequestCache;
use yew::prelude::*;

pub struct Application {
    access_code: Option<String>,
    task: Option<RequestHandle>,
    refresh_task: Option<Timeout>,
    token_holder: SharedDataBridge<Option<Token>>,
    /// Something failed, we can no longer work.
    app_failure: bool,
    /// We are in the process of authenticating.
    authenticating: bool,
    endpoints: Option<EndpointInformation>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Trigger fetching the endpoint information
    FetchBackend,
    /// Failed to fetch endpoint information
    FetchBackendFailed,
    /// Trigger an overall application failure
    AppFailure(Toast),
    /// Set the backend information
    Backend(BackendInformation),
    /// Set the endpoint information
    Endpoints(Rc<EndpointInformation>),
    /// Exchange the authentication code for an access token
    GetToken(String),
    /// Share the access token using the data bridge
    ShareAccessToken(Option<Token>),
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

impl Component for Application {
    type Message = Msg;
    type Properties = ();
    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::FetchBackend);

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
            ctx.link().send_message(Msg::AppFailure(Toast {
                title: "Failed to log in".into(),
                body: html! {<p>{error}</p>},
                r#type: Type::Danger,
                actions: vec![ctx
                    .link()
                    .callback(|_| Msg::RetryLogin)
                    .into_action("Retry")],
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

        let token_holder = SharedDataBridge::from(ctx.link(), Msg::SetAccessToken);

        Self {
            access_code: code,
            task: None,
            refresh_task: None,
            app_failure: false,
            authenticating: false,
            token_holder,
            endpoints: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        log::info!("Message: {:?}", msg);

        match msg {
            Msg::FetchBackend => {
                self.task = Some(
                    self.fetch_backend(ctx)
                        .expect("Failed to get backend information"),
                );
                true
            }
            Msg::Backend(backend) => {
                log::info!("Got backend: {:?}", backend);
                Backend::set(Some(backend));
                self.task = None;
                if !self.app_failure {
                    if let Some(access_code) = self.access_code.take() {
                        // exchange code for token if we have a code and no app failure
                        log::info!("Exchange access code for token");
                        self.authenticating = true;
                        ctx.link().send_message(Msg::GetToken(access_code));
                    } else if let Some(refresh) = Preferences::load()
                        .ok()
                        .and_then(|prefs| prefs.refresh_token)
                    {
                        log::info!("Re-using existing refresh token");
                        self.authenticating = true;
                        // try using existing refresh token
                        ctx.link().send_message(Msg::RefreshToken(Some(refresh)))
                    }
                }

                true
            }
            Msg::Endpoints(endpoints) => {
                log::info!("Got endpoints: {:?}", endpoints);
                self.endpoints =
                    Some(Rc::try_unwrap(endpoints).unwrap_or_else(|err| (*err).clone()));
                self.task = None;

                // we finished logging in the user
                self.authenticating = false;

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
                        self.fetch_token(ctx, &access_code)
                            .expect("Failed to create request"),
                    );
                } else {
                    self.access_code = Some(access_code);
                }
                true
            }
            Msg::ShareAccessToken(token) => {
                self.token_holder.set(token);
                false
            }
            Msg::SetAccessToken(Some(token)) => {
                log::info!("Token: {:?}", token);
                self.task = None;
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

                    let mut rem = timeout.as_secs() as i64;
                    if rem < 0 {
                        // ensure we are non-negative
                        rem = 0;
                    }

                    if rem < 30 {
                        // refresh now
                        log::debug!("Scheduling refresh now (had {} s remaining)", rem);
                        ctx.link()
                            .send_message(Msg::RefreshToken(token.refresh_token.as_ref().cloned()));
                    } else {
                        log::debug!("Scheduling refresh in {} seconds", rem);
                        let refresh_token = token.refresh_token.as_ref().cloned();
                        let delay = Duration::from_secs(rem as u64);
                        let link = ctx.link().clone();
                        self.refresh_task =
                            Some(Timeout::new(delay.as_millis() as u32, move || {
                                log::info!("Token timer expired, refreshing...");
                                link.send_message(Msg::RefreshToken(refresh_token))
                            }));
                    }
                } else {
                    log::debug!("Token has no expiration set");
                }

                // fetch endpoints

                if self.endpoints.is_none() {
                    self.task = Some(
                        self.fetch_endpoints(ctx)
                            .expect("Failed to fetch endpoints"),
                    );
                }

                // done

                true
            }
            Msg::SetAccessToken(None) => true,
            Msg::RefreshToken(refresh_token) => {
                log::info!("Refreshing access token");

                match refresh_token {
                    Some(refresh_token) => {
                        self.task = match self.refresh_token(ctx, &refresh_token) {
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! (
            <>
                <BackdropViewer/>
                <ToastViewer/>

                if let Some(ready) = self.is_ready() {

                    <Console
                        backend={ready.0}
                        token={ready.1}
                        endpoints={ready.2}
                        on_logout={ctx.link().callback(|_|Msg::Logout)}
                        />

                } else if let Some(backend) = self.need_login() {
                    <Placeholder info={backend.info} />
                }

            </>
        )
    }
}

impl Application {
    /// Check if the app and backend are ready to show the application.
    fn is_ready(&self) -> Option<(Backend, Token, EndpointInformation)> {
        match (
            self.app_failure,
            Backend::get(),
            Backend::token(),
            self.endpoints.clone(),
        ) {
            (true, ..) => None,
            (false, Some(backend), Some(token), Some(endpoints)) => {
                Some((backend, token, endpoints))
            }
            _ => None,
        }
    }

    fn need_login(&self) -> Option<Backend> {
        match (self.app_failure, Backend::get(), self.is_authenticating()) {
            (false, Some(backend), false) => Some(backend),
            _ => None,
        }
    }

    fn is_authenticating(&self) -> bool {
        self.authenticating || self.access_code.is_some()
    }

    fn fetch_backend(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(RequestBuilder::new(Method::GET, "/endpoints/backend.json")
            .cache(RequestCache::NoCache)
            .send(
                ctx.callback_json::<BackendInformation, Value, _>(|response| match response {
                    Ok(JsonResponse::Success(_, backend)) => Msg::Backend(backend),
                    _ => Msg::FetchBackendFailed,
                }),
            ))
    }

    fn fetch_endpoints(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(Backend::request_with(
            Method::GET,
            "/api/console/v1alpha1/info",
            Nothing,
            RequestOptions {
                disable_reauth: true,
            },
            ctx.callback_api::<Json<EndpointInformation>, _>(|response| match response {
                ApiResponse::Success(info, _) => Msg::Endpoints(Rc::new(info)),
                _ => Msg::FetchBackendFailed,
            }),
        )?)
    }

    fn refresh_token(
        &self,
        ctx: &Context<Self>,
        refresh_token: &str,
    ) -> Result<RequestHandle, anyhow::Error> {
        let mut url = Backend::url("/api/console/v1alpha1/ui/refresh")
            .ok_or_else(|| anyhow::anyhow!("Missing backend information"))?;

        url.query_pairs_mut()
            .append_pair("refresh_token", refresh_token);

        let req = RequestBuilder::new(Method::GET, url).cache(RequestCache::NoCache);

        Ok(req.send(
            ctx.callback_json::<Value, Value, _>(|response| Self::from_response(response, true)),
        ))
    }

    fn fetch_token<S: AsRef<str>>(
        &self,
        ctx: &Context<Self>,
        access_code: S,
    ) -> Result<RequestHandle, anyhow::Error> {
        let mut url = Backend::url("/api/console/v1alpha1/ui/token")
            .ok_or_else(|| anyhow::anyhow!("Missing backend information"))?;

        url.query_pairs_mut()
            .append_pair("code", access_code.as_ref());

        let req = RequestBuilder::new(Method::GET, url).cache(RequestCache::NoCache);

        Ok(req.send(ctx.callback_json(|response| Self::from_response(response, false))))
    }

    fn from_response(
        response: anyhow::Result<JsonResponse<Value, Value>>,
        is_refresh: bool,
    ) -> Msg {
        log::info!("Response from refreshing token: {:?}", response);
        match response {
            Ok(JsonResponse::Success(_, value)) => {
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
                    Some(token) => Msg::ShareAccessToken(Some(token)),
                    None => Msg::FetchTokenFailed,
                }
            }
            _ => Msg::FetchTokenFailed,
        }
    }
}
