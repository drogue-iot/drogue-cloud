mod admin;
mod debug;
mod integrations;

use super::{ApplicationTabs, Pages};
use crate::{
    backend::{ApiResponse, Backend, Json, JsonHandlerScopeExt, Nothing, RequestHandle, Token},
    console::AppRoute,
    error::{error, ErrorNotification, ErrorNotifier},
    html_prop,
    pages::{
        apps::{
            details::{admin::Admin, integrations::IntegrationDetails},
            DetailsSection,
        },
        HasReadyState,
    },
    utils::{to_yaml_model, url_encode},
};
use drogue_client::registry::v1::Application;
use drogue_cloud_console_common::EndpointInformation;
use http::Method;
use monaco::{api::*, sys::editor::BuiltinTheme, yew::CodeEditor};
use patternfly_yew::*;
use std::rc::Rc;
use yew::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub backend: Backend,
    pub token: Token,
    pub endpoints: EndpointInformation,
    pub name: String,
    pub details: DetailsSection,
}

#[derive(Debug)]
pub enum Msg {
    Load,
    Reset,
    SetData(Rc<Application>),
    Error(ErrorNotification),
    SaveEditor,
    SetAdmin(bool),
}

pub struct Details {
    fetch_task: Option<RequestHandle>,
    fetch_role: Option<RequestHandle>,

    content: Option<Rc<Application>>,
    yaml: Option<TextModel>,
    is_admin: bool,
}

impl Component for Details {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::Load);

        Self {
            content: None,
            yaml: None,
            fetch_task: None,
            fetch_role: None,
            is_admin: false,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => match self.load(ctx) {
                Ok((task, admin_task)) => {
                    self.fetch_task = Some(task);
                    self.fetch_role = Some(admin_task);
                }
                Err(err) => error("Failed to load", err),
            },
            Msg::SetData(content) => {
                self.content = Some(content);
                self.reset();
                self.fetch_task = None;
            }
            Msg::Reset => {
                self.reset();
            }
            Msg::SaveEditor => {
                if let Some(model) = &self.yaml {
                    let new_content = model.get_value();
                    match self.update_yaml(ctx, &new_content) {
                        Ok(task) => self.fetch_task = Some(task),
                        Err(err) => error("Failed to update", err),
                    }
                }
            }
            Msg::Error(msg) => {
                msg.toast();
                self.fetch_task = None;
            }
            Msg::SetAdmin(is_admin) => {
                self.fetch_role = None;
                self.is_admin = is_admin;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Content>
                        <Title>{ctx.props().name.clone()}</Title>
                    </Content>
                </PageSection>
            if let Some(app) = &self.content {
                { self.render_content(ctx, app) }
            } else {
                <PageSection><Grid></Grid></PageSection>
            }
            </>
        }
    }
}

impl Details {
    fn load(&self, ctx: &Context<Self>) -> Result<(RequestHandle, RequestHandle), anyhow::Error> {
        Ok((
            ctx.props().backend.info.request(
                Method::GET,
                format!(
                    "/api/registry/v1alpha1/apps/{}",
                    url_encode(&ctx.props().name)
                ),
                Nothing,
                vec![],
                ctx.callback_api::<Json<Application>, _>(move |response| match response {
                    ApiResponse::Success(content, _) => Msg::SetData(Rc::new(content)),
                    ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to load")),
                }),
            )?,
            ctx.props().backend.info.request(
                Method::GET,
                format!(
                    "/api/admin/v1alpha1/apps/{}/members",
                    url_encode(&ctx.props().name)
                ),
                Nothing,
                vec![],
                ctx.callback_api::<(), _>(move |response| match response {
                    ApiResponse::Success(..) => Msg::SetAdmin(true),
                    ApiResponse::Failure(..) => Msg::SetAdmin(false),
                }),
            )?,
        ))
    }

    fn update(
        &self,
        ctx: &Context<Self>,
        app: Application,
    ) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.info.request(
            Method::PUT,
            format!(
                "/api/registry/v1alpha1/apps/{}",
                url_encode(&ctx.props().name)
            ),
            Json(&app),
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(..) => Msg::Load,
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to update")),
            }),
        )?)
    }

    fn update_yaml(&self, ctx: &Context<Self>, yaml: &str) -> Result<RequestHandle, anyhow::Error> {
        let app = serde_yaml::from_str(yaml)?;
        log::info!("Updating to: {:#?}", app);
        self.update(ctx, app)
    }

    fn reset(&mut self) {
        if let Some(content) = &self.content {
            self.yaml = to_yaml_model(content.as_ref()).ok();
        } else {
            self.yaml = None;
        }
    }

    fn render_content(&self, ctx: &Context<Self>, app: &Application) -> Html {
        let name = app.metadata.name.clone();
        let transformer = SwitchTransformer::new(
            |global| match global {
                AppRoute::Applications(Pages::Details {
                    name: _name,
                    details,
                }) => Some(details),
                _ => None,
            },
            move |local| {
                AppRoute::Applications(Pages::Details {
                    name: name.clone(),
                    details: local,
                })
            },
        );

        let mut tabs = Vec::new();
        tabs.push(html_nested! {
           <TabRouterItem<DetailsSection> to={DetailsSection::Overview} label="Overview"/>
        });
        tabs.push(html_nested! {
           <TabRouterItem<DetailsSection> to={DetailsSection::Integrations} label="Integrations"/>
        });
        tabs.push(html_nested! {
           <TabRouterItem<DetailsSection> to={DetailsSection::Yaml} label="YAML"/>
        });
        tabs.push(html_nested! {
           <TabRouterItem<DetailsSection> to={DetailsSection::Debug} label="Debug"/>
        });
        if self.is_admin && self.fetch_role.is_none() {
            tabs.push(html_nested!{
           <TabRouterItem<DetailsSection> to={DetailsSection::Administration} label="Administration"/>
        });
        }

        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light}>
                    <ApplicationTabs
                        transformer={transformer}
                        >
                        { tabs }
                    </ApplicationTabs>
                </PageSection>
                <PageSection>
                {
                    match ctx.props().details {
                        DetailsSection::Overview => self.render_overview(app),
                        DetailsSection::Integrations => self.render_integrations(ctx, app),
                        DetailsSection::Yaml => self.render_editor(ctx),
                        DetailsSection::Debug => self.render_debug(ctx, app),
                        DetailsSection::Administration => self.render_admin(ctx, ),
                    }
                }
                </PageSection>
            </>
        };
    }

    fn render_overview(&self, app: &Application) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols={[3]}>
                    <Card
                        title={html_prop!({"Details"})}
                        >
                        <DescriptionList>
                            <DescriptionGroup term="Name">
                                {&app.metadata.name}
                            </DescriptionGroup>
                            <DescriptionGroup term="Labels">
                                { for app.metadata.labels.iter().map(|(k,v)|
                                    if v.is_empty() {
                                        html!{ <Label label={k.clone()}/>}
                                    } else {
                                        html!{ <Label label={format!("{}={}", k, v)}/>}
                                    }
                                ) }
                            </DescriptionGroup>
                        </DescriptionList>
                    </Card>
                </GridItem>
                <GridItem cols={[3]}>
                    <Card
                        title={html_prop!({"Kafka"})}
                        >
                        <DescriptionList>
                            <DescriptionGroup term="State">
                                {app.render_condition("KafkaReady")}
                            </DescriptionGroup>
                        </DescriptionList>
                    </Card>
                </GridItem>
            </Grid>
        };
    }

    fn render_admin(&self, ctx: &Context<Self>) -> Html {
        // create the Admin component, using a copy of the same props.
        return html! {
            <Admin
                backend={ctx.props().backend.clone()}
                token={ctx.props().token.clone()}
                endpoints={ctx.props().endpoints.clone()}
                name={ctx.props().name.clone()}
                details={ctx.props().details.clone()}
            />
        };
    }

    fn render_integrations(&self, ctx: &Context<Self>, application: &Application) -> Html {
        IntegrationDetails {
            backend: &ctx.props().backend,
            application,
            token: &ctx.props().token,
            endpoints: &ctx.props().endpoints,
        }
        .render()
    }

    fn render_debug(&self, ctx: &Context<Self>, application: &Application) -> Html {
        return html! {
            <debug::Debug
                backend={ctx.props().backend.clone()}
                application={application.metadata.name.clone()}
                endpoints={ctx.props().endpoints.clone()}
                token={ctx.props().token.clone()}
                />
        };
    }

    fn render_editor(&self, ctx: &Context<Self>) -> Html {
        let options = CodeEditorOptions::default()
            .with_scroll_beyond_last_line(false)
            .with_language("yaml".to_owned())
            .with_builtin_theme(BuiltinTheme::VsDark);

        let options = Rc::new(options);

        return html! {
            <>
            <Stack>
                <StackItem fill=true>
                    <CodeEditor model={self.yaml.clone()} options={options}/>
                </StackItem>
                <StackItem>
                    <Form>
                    <ActionGroup>
                        <Button disabled={self.fetch_task.is_some()} label="Save" variant={Variant::Primary} onclick={ctx.link().callback(|_|Msg::SaveEditor)}/>
                        <Button disabled={self.fetch_task.is_some()} label="Reload" variant={Variant::Secondary} onclick={ctx.link().callback(|_|Msg::Load)}/>
                        <Button disabled={self.fetch_task.is_some()} label="Cancel" variant={Variant::Secondary} onclick={ctx.link().callback(|_|Msg::Reset)}/>
                    </ActionGroup>
                    </Form>
                </StackItem>
            </Stack>
            </>
        };
    }
}
