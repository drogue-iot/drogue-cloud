mod commands;
mod consume;
mod data;
mod publish;
mod register;

pub use commands::*;
pub use consume::*;
pub use publish::*;
pub use register::*;

use crate::{
    backend::{Backend, Token},
    data::SharedDataBridge,
    examples::data::ExampleData,
};
use anyhow::Error;
use data::CoreExampleData;
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use std::rc::Rc;
use yew::{
    format::{Json, Nothing},
    prelude::*,
    services::fetch::*,
};
use yew_router::prelude::*;

#[derive(Switch, Debug, Clone, PartialEq, Eq)]
pub enum Examples {
    #[to = "/register"]
    Register,
    #[to = "/consume"]
    Consume,
    #[to = "/publish"]
    Publish,
    #[to = "/commands"]
    Commands,
}

impl Examples {
    pub fn title(&self) -> String {
        match self {
            Self::Register => "Registering devices".into(),
            Self::Consume => "Consuming data".into(),
            Self::Publish => "Publishing data".into(),
            Self::Commands => "Command & control".into(),
        }
    }
}

pub fn shell_quote<S: ToString>(s: S) -> String {
    s.to_string().replace('\\', "\\\\").replace('\'', "\\\'")
}

/// Escape into single-quote string
pub fn shell_single_quote<S: ToString>(s: S) -> String {
    let s = s.to_string().replace('\'', r#"'"'"'"#);
    format!("'{}'", s)
}

pub fn url_encode<S: AsRef<str>>(s: S) -> String {
    percent_encoding::utf8_percent_encode(s.as_ref(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
}

#[derive(Clone, Debug, PartialEq, Eq, Properties)]
pub struct Props {
    pub example: Examples,
}

pub struct ExamplePage {
    props: Props,
    link: ComponentLink<Self>,

    ft: Option<FetchTask>,
    endpoints: Option<Endpoints>,

    data: Option<ExampleData>,
    _data_agent: SharedDataBridge<ExampleData>,

    token: Option<Token>,
    _token_agent: SharedDataBridge<Option<Token>>,
}

#[derive(Clone, Debug)]
pub enum Msg {
    FetchOverview,
    FetchOverviewFailed,
    OverviewUpdate(Rc<Endpoints>),

    ExampleData(ExampleData),
    SetToken(Option<Token>),
}

impl Component for ExamplePage {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut data_agent = SharedDataBridge::from(&link, Msg::ExampleData);
        data_agent.request_state();

        let mut token_agent = SharedDataBridge::from(&link, Msg::SetToken);
        token_agent.request_state();

        link.send_message(Msg::FetchOverview);

        Self {
            props,
            link,

            ft: None,
            endpoints: None,

            data: None,
            _data_agent: data_agent,

            token: None,
            _token_agent: token_agent,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::FetchOverview => {
                self.ft = Some(self.fetch_overview().unwrap());
            }
            Msg::FetchOverviewFailed => return false,
            Msg::OverviewUpdate(e) => {
                self.endpoints = Some(e.as_ref().clone());
            }
            Msg::ExampleData(data) => {
                self.data = Some(data);
            }
            Msg::SetToken(token) => {
                self.token = token;
            }
        }
        true
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.props != props {
            self.props = props;
            true
        } else {
            false
        }
    }

    fn view(&self) -> Html {
        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Getting started"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    { self.render_overview() }
                </PageSection>
            </>
        };
    }
}

impl ExamplePage {
    fn fetch_overview(&self) -> Result<FetchTask, Error> {
        Backend::request(
            Method::GET,
            "/api/v1/info",
            Nothing,
            self.link
                .callback(|response: Response<Json<Result<Endpoints, Error>>>| {
                    let parts = response.into_parts();
                    if let (meta, Json(Ok(body))) = parts {
                        if meta.status.is_success() {
                            return Msg::OverviewUpdate(Rc::new(body));
                        }
                    }
                    Msg::FetchOverviewFailed
                }),
        )
    }

    fn render_overview(&self) -> Html {
        match (&self.endpoints, &self.data, &self.token) {
            (Some(endpoints), Some(data), Some(token)) => {
                self.render_main(endpoints.clone(), data.clone(), token.clone())
            }
            _ => html! {
                <div>{"Loading..."}</div>
            },
        }
    }

    fn render_main(&self, endpoints: Endpoints, data: ExampleData, token: Token) -> Html {
        return html! {
            <Grid gutter=true>

                <GridItem
                    cols=[10]
                    >
                    <Stack gutter=true>

                        <StackItem>
                            <Title size=Size::XXLarge>{self.props.example.title()}</Title>
                        </StackItem>

                        {
                            match self.props.example {
                                Examples::Register => html!{
                                    <RegisterDevices
                                        endpoints=endpoints.clone()
                                        data=data.clone()
                                        />
                                },
                                Examples::Consume => html!{
                                    <ConsumeData
                                        endpoints=endpoints.clone()
                                        data=data.clone()
                                        token=token.clone()
                                        />
                                },
                                Examples::Publish => html!{
                                    <PublishData
                                        endpoints=endpoints.clone()
                                        data=data.clone()
                                        />
                                },
                                Examples::Commands => html!{
                                    <CommandAndControl
                                        endpoints=endpoints.clone()
                                        data=data.clone()
                                        token=token.clone()
                                        />
                                },
                            }
                        }

                    </Stack>
                </GridItem>

                <GridItem cols=[2]>
                    <CoreExampleData/>
                </GridItem>

            </Grid>
        };
    }
}
