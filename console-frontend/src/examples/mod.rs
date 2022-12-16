mod commands;
mod consume;
mod data;
mod publish;
mod register;

pub use commands::*;
pub use consume::*;
pub use publish::*;
pub use register::*;

use crate::utils::context::MutableContext;
use crate::{
    backend::{
        ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, Nothing, RequestHandle,
    },
    examples::data::ExampleData,
    utils::context::ContextListener,
};
use anyhow::Error;
use data::CoreExampleData;
use drogue_cloud_service_api::endpoints::Endpoints;
use http::Method;
use patternfly_yew::*;
use std::rc::Rc;
use yew::prelude::*;
use yew_nested_router::prelude::*;
use yew_oauth2::prelude::*;

#[derive(Target, Debug, Clone, PartialEq, Eq)]
pub enum Examples {
    Register,
    Consume,
    Publish,
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

#[derive(Clone, Debug, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub example: Examples,
}

pub struct ExamplePage {
    ft: Option<RequestHandle>,
    endpoints: Option<Endpoints>,

    auth: ContextListener<OAuth2Context>,
    data: MutableContext<ExampleData>,
}

pub enum Msg {
    FetchOverview,
    FetchOverviewFailed,
    OverviewUpdate(Rc<Endpoints>),
    SetExampleData(Box<dyn FnOnce(&mut ExampleData)>),
}

impl Component for ExamplePage {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::FetchOverview);

        Self {
            ft: None,
            endpoints: None,
            data: MutableContext::new(
                ExampleData::default(),
                ctx.link().callback(Msg::SetExampleData),
            ),
            auth: ContextListener::unwrap(ctx),
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
            Msg::SetExampleData(mutator) => {
                return self.data.apply(mutator);
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let context = self.data.clone();
        html! (
            <>
                <ContextProvider<MutableContext<ExampleData>> {context}>
                    <PageSection variant={PageSectionVariant::Light} limit_width=true>
                        <Content>
                            <h1>{"Getting started"}</h1>
                        </Content>
                    </PageSection>
                    <PageSection>
                        { self.render_overview(ctx) }
                    </PageSection>
                </ContextProvider<MutableContext<ExampleData>>>
            </>
        )
    }
}

impl ExamplePage {
    fn fetch_overview(&self, ctx: &Context<Self>) -> Result<RequestHandle, Error> {
        Ok(ctx.props().backend.request(
            Method::GET,
            "/api/console/v1alpha1/info",
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<Json<Endpoints>, _>(|response| match response {
                ApiResponse::Success(endpoints, _) => Msg::OverviewUpdate(Rc::new(endpoints)),
                ApiResponse::Failure(_) => Msg::FetchOverviewFailed,
            }),
        )?)
    }

    fn render_overview(&self, ctx: &Context<Self>) -> Html {
        match (&self.endpoints, self.auth.get().authentication()) {
            (Some(endpoints), Some(auth)) => {
                self.render_main(ctx, endpoints.clone(), &self.data.context, auth.clone())
            }
            _ => html! (
                <div>{"Loading..."}</div>
            ),
        }
    }

    fn render_main(
        &self,
        ctx: &Context<Self>,
        endpoints: Endpoints,
        data: &ExampleData,
        auth: Authentication,
    ) -> Html {
        html! (
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
                                        {auth}
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
                                        {auth}
                                    />
                                },
                            }
                        }

                    </Stack>
                </GridItem>

                <GridItem cols={[2]}>
                    <CoreExampleData endpoints={endpoints} />
                </GridItem>

            </Grid>
        )
    }
}

fn note_local_certs(local_certs: bool) -> Html {
    match local_certs {
        true => html! (
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
        ),
        false => html!(),
    }
}
