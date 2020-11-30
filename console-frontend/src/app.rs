use crate::backend::{Backend, BackendInformation, Token};
use crate::index::Index;
use crate::placeholder::Placeholder;
use crate::spy::Spy;
use anyhow::Error;
use chrono::{DateTime, Utc};
use patternfly_yew::*;
use std::time::Duration;
use url::Url;
use yew::{
    format::{Json, Nothing},
    prelude::*,
    services::{fetch::*, storage::*, timeout::*},
    utils::window,
};
use yew_router::prelude::*;

#[derive(Switch, Debug, Clone, PartialEq)]
pub enum AppRoute {
    #[to = "/spy"]
    Spy,
    #[to = "/"]
    Index,
}

pub struct Main {
    link: ComponentLink<Self>,
    access_code: Option<String>,
    storage: StorageService,
    task: Option<FetchTask>,
    refresh_task: Option<TimeoutTask>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    FetchEndpoint,
    FetchFailed,
    Endpoint(BackendInformation),
    SetCode(String),
    GetToken,
    SetAccessToken(Token),
    LoginFailed,
    // send to trigger refreshing the access token
    RefreshToken,
}

impl Component for Main {
    type Message = Msg;
    type Properties = ();
    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchEndpoint);

        let storage = StorageService::new(Area::Session).expect("storage was disabled by the user");

        let location = window().location();
        let url = Url::parse(&location.href().unwrap()).unwrap();

        log::info!("href: {:?}", url);

        let code = url
            .query_pairs()
            .find_map(|(k, v)| if k == "code" { Some(v) } else { None })
            .map(|s| s.to_string());

        log::info!("Access code: {:?}", code);

        if let Some(code) = code {
            link.send_message(Msg::SetCode(code));
        }

        // remove code, state and others from the URL bar
        //  window().location().set_search("").ok();

        Self {
            link,
            access_code: None,
            storage,
            task: None,
            refresh_task: None,
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
                self.link.send_message(Msg::GetToken);
                if self.access_code.is_none() {
                    // we have no code yet, re-auth
                    Backend::reauthenticate();
                }
                true
            }
            Msg::FetchFailed => false,
            Msg::LoginFailed => {
                Backend::update_token(None);
                Backend::reauthenticate();
                // FIXME: need to show some notification
                true
            }
            Msg::SetCode(code) => {
                // got code, convert to access token
                self.access_code = Some(code.clone());
                self.link.send_message(Msg::GetToken);
                true
            }
            Msg::GetToken => {
                // get the access token from the code
                // this can only be called once the backend information and the access code is available
                if Backend::get().is_some() && self.access_code.is_some() {
                    self.task = Some(self.fetch_token().expect("Failed to create request"));
                }
                true
            }
            Msg::SetAccessToken(token) => {
                log::info!("Token: {:?}", token);
                self.task = None;
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
                        log::info!("Scheduling refresh now (had {} s remaining)", rem);
                        self.link.send_message(Msg::RefreshToken);
                    } else {
                        log::info!("Scheduling refresh in {} seconds", rem);
                        self.refresh_task = Some(TimeoutService::spawn(
                            Duration::from_secs(rem as u64),
                            self.link.callback(|_| {
                                log::info!("Token timer expired, refreshing...");
                                Msg::RefreshToken
                            }),
                        ));
                    }
                } else {
                    log::info!("Token has no expiration set");
                }
                true
            }
            Msg::RefreshToken => {
                log::info!("Refreshing access token");

                match Backend::token().and_then(|t| t.refresh_token) {
                    Some(refresh_token) => {
                        self.task = match self.refresh_token(&refresh_token) {
                            Ok(task) => Some(task),
                            Err(_) => {
                                Backend::reauthenticate();
                                None
                            }
                        }
                    }
                    None => {
                        Backend::reauthenticate();
                    }
                }

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
                    if self.is_ready() {
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
    /// Check if the app and backend are ready to show the application.
    fn is_ready(&self) -> bool {
        Backend::get().is_some() && Backend::access_token().is_some()
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
                    Msg::FetchFailed
                },
            ),
        )
    }

    fn refresh_token(&self, refresh_token: &str) -> Result<FetchTask, anyhow::Error> {
        let mut url = Backend::url("/ui/refresh")
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
                    Self::from_response(response)
                },
            ),
        )
    }

    fn fetch_token(&self) -> Result<FetchTask, anyhow::Error> {
        let mut url = Backend::url("/ui/token")
            .ok_or_else(|| anyhow::anyhow!("Missing backend information"))?;

        url.query_pairs_mut().append_pair(
            "code",
            &self
                .access_code
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing access code"))?,
        );

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
                    Self::from_response(response)
                },
            ),
        )
    }

    fn from_response(response: Response<Json<Result<serde_json::Value, Error>>>) -> Msg {
        if let (meta, Json(Ok(value))) = response.into_parts() {
            if meta.status.is_success() {
                let access_token = value["bearer"]["access_token"]
                    .as_str()
                    .map(|s| s.to_string());
                let refresh_token = value["bearer"]["refresh_token"]
                    .as_str()
                    .map(|s| s.to_string());

                let expires = match value["expires"].as_str() {
                    Some(expires) => DateTime::parse_from_rfc3339(expires).ok(),
                    None => None,
                }
                .map(|expires| expires.with_timezone(&Utc));
                let token = access_token.map(|access_token| Token {
                    access_token,
                    refresh_token,
                    expires,
                });
                log::info!("Token: {:?}", token);
                match token {
                    Some(token) => Msg::SetAccessToken(token),
                    None => Msg::LoginFailed,
                }
            } else {
                Msg::LoginFailed
            }
        } else {
            Msg::LoginFailed
        }
    }
}
