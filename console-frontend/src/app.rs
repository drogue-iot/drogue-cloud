use crate::backend::{Backend, BackendInformation};
use crate::index::Index;
use crate::placeholder::Placeholder;
use crate::spy::Spy;
use anyhow::Error;
use patternfly_yew::*;
use url::Url;
use yew::{
    format::{Json, Nothing},
    prelude::*,
    services::fetch::*,
    services::storage::*,
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
            .find_map(|(k, v)| if k == "code" { Some(v) } else { None });

        if let Some(code) = code {
            let code = code.to_string();
            log::info!("Code: {}", code);
            storage.store("code", Ok(code));
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
