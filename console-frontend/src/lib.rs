#![recursion_limit = "512"]

mod components;
mod index;
mod placeholder;
mod spy;

use patternfly_yew::*;

use wasm_bindgen::prelude::*;

use yew::prelude::*;
use yew::services::fetch::*;

use yew_router::prelude::*;

use crate::index::Index;
use crate::placeholder::Placeholder;
use crate::spy::Spy;

use yew::format::{Json, Nothing};

use anyhow::Error;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Backend {
    pub url: String,
}

static CONSOLE_BACKEND: Lazy<RwLock<Option<Backend>>> = Lazy::new(|| RwLock::new(None));

impl Backend {
    /// Return the backend endpoint, or [`Option::None`].
    pub fn get() -> Option<Backend> {
        CONSOLE_BACKEND.read().unwrap().clone()
    }
    fn set(backend: Option<Backend>) {
        *CONSOLE_BACKEND.write().unwrap() = backend;
    }
}

#[derive(Switch, Debug, Clone, PartialEq)]
pub enum AppRoute {
    #[to = "/spy"]
    Spy,
    #[to = "/"]
    Index,
}

struct Main {
    link: ComponentLink<Self>,
    task: Option<FetchTask>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    FetchEndpoint,
    FetchFailed,
    Endpoint(Backend),
}

impl Component for Main {
    type Message = Msg;
    type Properties = ();
    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchEndpoint);
        Self { link, task: None }
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
                true
            }
            Msg::FetchFailed => false,
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
                    if Backend::get().is_some() {
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

        FetchService::fetch(
            req,
            self.link
                .callback(|response: Response<Json<Result<Backend, Error>>>| {
                    log::info!("Backend: {:?}", response);
                    if let (meta, Json(Ok(body))) = response.into_parts() {
                        if meta.status.is_success() {
                            return Msg::Endpoint(body);
                        }
                    }
                    Msg::FetchFailed
                }),
        )
    }
}

#[wasm_bindgen(start)]
pub fn run_app() {
    wasm_logger::init(Default::default());
    log::info!("Getting ready...");
    App::<Main>::new().mount_to_body();
}
