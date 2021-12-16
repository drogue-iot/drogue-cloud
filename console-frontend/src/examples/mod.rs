mod commands;
mod consume;
mod data;
mod publish;
mod register;

pub use commands::*;
pub use consume::*;
pub use publish::*;
pub use register::*;

use crate::backend::{ApiResponse, Json, JsonHandlerScopeExt, Nothing, RequestHandle};
use crate::{
    backend::{Backend, Token},
    data::SharedDataBridge,
    examples::data::ExampleData,
};
use anyhow::Error;
use data::CoreExampleData;
use drogue_cloud_service_api::endpoints::Endpoints;
use http::Method;
use patternfly_yew::*;
use std::rc::Rc;
use yew::prelude::*;
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

#[derive(Clone, Debug, PartialEq, Eq, Properties)]
pub struct Props {
    pub example: Examples,
}

pub struct ExamplePage {
    ft: Option<RequestHandle>,
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

    fn create(ctx: &Context<Self>) -> Self {
        let mut data_agent = SharedDataBridge::from(&ctx.link(), Msg::ExampleData);
        data_agent.request_state();

        let mut token_agent = SharedDataBridge::from(&ctx.link(), Msg::SetToken);
        token_agent.request_state();

        ctx.link().send_message(Msg::FetchOverview);

        Self {
            ft: None,
            endpoints: None,

            data: None,
            _data_agent: data_agent,

            token: None,
            _token_agent: token_agent,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::FetchOverview => {
                self.ft = Some(self.fetch_overview(ctx).unwrap());
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Content>
                        <h1>{"Getting started"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    { self.render_overview(ctx) }
                </PageSection>
            </>
        };
    }
}

impl ExamplePage {
    fn fetch_overview(&self, ctx: &Context<Self>) -> Result<RequestHandle, Error> {
        Ok(Backend::request(
            Method::GET,
            "/api/console/v1alpha1/info",
            Nothing,
            ctx.callback_api::<Json<Endpoints>, _>(|response| match response {
                ApiResponse::Success(endpoints, _) => Msg::OverviewUpdate(Rc::new(endpoints)),
                ApiResponse::Failure(_) => Msg::FetchOverviewFailed,
            }),
        )?)
    }

    fn render_overview(&self, ctx: &Context<Self>) -> Html {
        match (&self.endpoints, &self.data, &self.token) {
            (Some(endpoints), Some(data), Some(token)) => {
                self.render_main(ctx, endpoints.clone(), data.clone(), token.clone())
            }
            _ => html! {
                <div>{"Loading..."}</div>
            },
        }
    }

    fn render_main(
        &self,
        ctx: &Context<Self>,
        endpoints: Endpoints,
        data: ExampleData,
        token: Token,
    ) -> Html {
        return html! {
            <Grid gutter=true>

                <GridItem
                    cols={[10]}
                    >
                    <Stack gutter=true>

                        <StackItem>
                            <Title size={Size::XXLarge}>{ctx.props().example.title()}</Title>
                        </StackItem>

                        {
                            match ctx.props().example {
                                Examples::Register => html!{
                                    <RegisterDevices
                                        endpoints={endpoints.clone()}
                                        data={data.clone()}
                                        />
                                },
                                Examples::Consume => html!{
                                    <ConsumeData
                                        endpoints={endpoints.clone()}
                                        data={data.clone()}
                                        token={token.clone()}
                                        />
                                },
                                Examples::Publish => html!{
                                    <PublishData
                                        endpoints={endpoints.clone()}
                                        data={data.clone()}
                                        />
                                },
                                Examples::Commands => html!{
                                    <CommandAndControl
                                        endpoints={endpoints.clone()}
                                        data={data.clone()}
                                        token={token.clone()}
                                        />
                                },
                            }
                        }

                    </Stack>
                </GridItem>

                <GridItem cols={[2]}>
                    <CoreExampleData
                        endpoints={endpoints.clone()}
                        />
                </GridItem>

            </Grid>
        };
    }
}

fn note_local_certs(local_certs: bool) -> Html {
    match local_certs {
        true => html! {
            <Alert  r#type={Type::Warning} title="Check your path!" inline=true>
                <Content>
                    <p>{r#"
                    This command uses the locally generated certificate bundle. The command will fail if you are not executing it from the root directory of the installer or repository."#}
                    </p><p>
                    {r#"
                    Alternatively, you may adapt the path to the "#} <code> {"root-cert.pem"}</code> {r#"file in the command yourself.
                    "#}</p>
                </Content>
            </Alert>
        },
        false => html! {},
    }
}
