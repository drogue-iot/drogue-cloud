use crate::{backend::BackendInformation, components::placeholder::Placeholder, console::Console};
use async_trait::async_trait;
use drogue_client::error::ErrorInformation;
use drogue_cloud_console_common::EndpointInformation;
use futures_util::future::TryFutureExt;
use gloo_utils::window;
use http::header;
use patternfly_yew::*;
use reqwest::Client;
use serde::Deserialize;
use url::Url;
use wasm_bindgen_futures::spawn_local;
use yew::{html::IntoPropValue, prelude::*};
use yew_oauth2::{openid::*, prelude::*};

#[derive(Clone, Debug)]
pub struct LoginInformation {
    backend: BackendInformation,
    endpoints: EndpointInformation,
}

impl IntoPropValue<openid::Config> for &LoginInformation {
    fn into_prop_value(self) -> openid::Config {
        openid::Config {
            client_id: self.backend.openid.client_id.clone(),
            issuer_url: self.backend.openid.issuer_url.clone(),
            additional: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Properties)]
pub struct AppMainProps {
    info: LoginInformation,
}

#[function_component(AppMain)]
fn app_main(props: &AppMainProps) -> Html {
    let agent = use_auth_agent().expect("Must be nested under the OAuth2 component");

    let logout = Callback::from(move |_| agent.logout());

    let info = &props.info;

    html!(
        <>
            <Authenticated>
                <ContextProvider<SharedData<ExampleData>>>
                    <Console
                        backend={info.backend.clone()}
                        endpoints={info.endpoints.clone()}
                        on_logout={logout}
                        />
                </ContextProvider<SharedData<ExampleData>>>
            </Authenticated>
            <NotAuthenticated>
                <Placeholder info={info.backend.clone()} />
            </NotAuthenticated>
        </>
    )
}

pub struct Application {
    login_info: Option<Result<LoginInformation, BackendError>>,
}

#[derive(Debug)]
pub enum Msg {
    /// Set client
    SetInfo(Result<LoginInformation, BackendError>),
}

impl Component for Application {
    type Message = Msg;
    type Properties = ();
    fn create(ctx: &Context<Self>) -> Self {
        Self::fetch_client(ctx.link().callback(Msg::SetInfo));
        Self { login_info: None }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        log::info!("Message: {:?}", msg);

        match msg {
            Msg::SetInfo(info) => {
                self.login_info = Some(info);
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! (
            <>
                <BackdropViewer>
                    <ToastViewer>

                {
                    match &self.login_info {
                        Some(Ok(info)) => {
                            html!(
                                <OAuth2
                                    config={info}
                                    >
                                    <AppMain {info}/>
                                </OAuth2>
                            )
                        },
                        Some(Err(err)) => {
                            html!(<>
                                <h1>{ "OAuth2 client error" } </h1>
                                <div>
                                    { err }
                                </div>
                            </>)
                        },
                        None => {
                            html!()
                        },
                    }
                }

                    </ToastViewer>
                </BackdropViewer>
            </>
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("Error: {0}")]
    Generic(String),
    #[error("Request error")]
    Request(#[from] reqwest::Error),
    #[error("Error response: {0}")]
    Response(ErrorInformation),
    #[error("Unknown response")]
    UnknownResponse,
}

#[async_trait(?Send)]
pub trait ResponseExt {
    async fn client_response<T>(self) -> Result<T, BackendError>
    where
        T: for<'de> Deserialize<'de>;
}

#[async_trait(?Send)]
impl ResponseExt for Result<reqwest::Response, reqwest::Error> {
    async fn client_response<T>(self) -> Result<T, BackendError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let resp = self?;

        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else if resp.status().is_client_error() || resp.status().is_server_error() {
            Err(BackendError::Response(resp.json().await?))
        } else {
            Err(BackendError::UnknownResponse)
        }
    }
}

async fn fetch_info(client: Client) -> Result<BackendInformation, BackendError> {
    let mut url = window()
        .location()
        .href()
        .map_err(|err| {
            BackendError::Generic(format!(
                "Unable to get base URL: {0}",
                err.as_string().unwrap_or_else(|| "<unknown>".to_string())
            ))
        })
        .and_then(|url| {
            Url::parse(&url)
                .map_err(|err| BackendError::Generic(format!("Unable to parse base URL: {err}")))
        })?;

    url.set_path("/endpoints/backend.json");
    url.query_pairs_mut().clear();

    log::info!("Fetch backend info: {url}");

    let backend: BackendInformation = client
        .get(url)
        .header(header::CACHE_CONTROL, "no-cache")
        .send()
        .await
        .client_response()
        .await?;

    Ok(backend)
}

async fn fetch_login(
    client: Client,
    backend: BackendInformation,
) -> Result<LoginInformation, BackendError> {
    let endpoints: EndpointInformation = client
        .get(backend.url("/.well-known/drogue-endpoints"))
        .header(header::CACHE_CONTROL, "no-cache")
        .send()
        .await
        .client_response()
        .await?;

    Ok(LoginInformation { backend, endpoints })
}

impl Application {
    fn fetch_client(callback: Callback<Result<LoginInformation, BackendError>>) {
        spawn_local(async move {
            let client = Client::new();
            let login = fetch_info(client.clone())
                .and_then(|info| fetch_login(client, info))
                .await;
            callback.emit(login);
        });
    }
}
