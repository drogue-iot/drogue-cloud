use super::Pages;
use crate::backend::{
    ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, Nothing, RequestHandle,
};
use crate::{
    console::AppRoute,
    error::{error, ErrorNotification, ErrorNotifier},
    html_prop,
    pages::{
        apps::{self, ApplicationContext},
        devices::{debug, delete::DeleteConfirmation, CloneDialog, DetailsSection},
    },
    utils::{context::ContextListener, url_encode},
};
use drogue_client::registry::v1::Device;
use drogue_cloud_console_common::EndpointInformation;
use http::Method;
use monaco::{api::*, sys::editor::BuiltinTheme, yew::CodeEditor};
use patternfly_yew::*;
use std::{ops::Deref, rc::Rc};
use yew::prelude::*;
use yew_nested_router::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub endpoints: EndpointInformation,
    pub app: String,
    pub name: String,
    pub details: DetailsSection,
}

pub enum Msg {
    Load,
    Reset,
    SetData(Rc<Device>),
    Error(ErrorNotification),
    SaveEditor,
    Delete,
    Clone,
    ShowApp(String),
}

pub struct Details {
    fetch_task: Option<RequestHandle>,

    backdropper: ContextListener<Backdropper>,
    toaster: ContextListener<Toaster>,
    router: ContextListener<RouterContext<AppRoute>>,

    content: Option<Rc<Device>>,
    yaml: Option<TextModel>,
}

impl Component for Details {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::Load);

        Self {
            backdropper: ContextListener::unwrap(ctx),
            toaster: ContextListener::unwrap(ctx),
            router: ContextListener::unwrap(ctx),
            content: None,
            yaml: None,
            fetch_task: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => match self.load(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error(&self.toaster.get(), "Failed to load", err),
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
                        Err(err) => error(&self.toaster.get(), "Failed to update", err),
                    }
                }
            }
            Msg::Error(msg) => {
                msg.toast(&self.toaster.get());
                self.fetch_task = None;
            }
            Msg::Delete => self.backdropper.get().open(html! {
                <DeleteConfirmation
                    backend={ctx.props().backend.clone()}
                    name={ctx.props().name.clone()}
                    app_name={ctx.props().app.clone()}
                    on_close={ctx.link().callback(move |_| Msg::Load)}
                />
            }),
            Msg::Clone => self.backdropper.get().open(html! {
               <CloneDialog
                    backend={ctx.props().backend.clone()}
                    data={self.content.as_ref().unwrap().as_ref().clone()}
                    app={ctx.props().app.clone()}
                    on_close={ctx.link().callback(move |_| Msg::Load)}
                />
            }),
            Msg::ShowApp(app) => {
                self.router
                    .get()
                    .push(AppRoute::Applications(apps::Pages::Details {
                        name: app,
                        details: apps::DetailsSection::Overview,
                    }))
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Content>
                        <Flex>
                            <FlexItem>
                                <Title>{ctx.props().name.clone()}</Title>
                            </FlexItem>
                            <FlexItem modifiers={[FlexModifier::Align(Alignement::Right).all()]}>
                                <Button
                                        label="Clone"
                                        disabled={self.content.is_none()}
                                        variant={Variant::Secondary}
                                        onclick={ctx.link().callback(|_|Msg::Clone)}
                                />
                            </FlexItem>
                            <FlexItem>
                                <Button
                                        label="Delete"
                                        variant={Variant::DangerSecondary}
                                        onclick={ctx.link().callback(|_|Msg::Delete)}
                                />
                            </FlexItem>
                        </Flex>
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
    fn load(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::GET,
            format!(
                "/api/registry/v1alpha1/apps/{}/devices/{}",
                url_encode(&ctx.props().app),
                url_encode(&ctx.props().name)
            ),
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<Json<Device>, _>(move |response| match response {
                ApiResponse::Success(device, _) => Msg::SetData(Rc::new(device)),
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to load")),
            }),
        )?)
    }

    fn update(&self, ctx: &Context<Self>, app: Device) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::PUT,
            format!(
                "/api/registry/v1alpha1/apps/{}/devices/{}",
                url_encode(&ctx.props().app),
                url_encode(&ctx.props().name)
            ),
            vec![],
            Json(app),
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
            let yaml = serde_yaml::to_string(content.as_ref()).unwrap_or_default();
            let p: &[_] = &['-', '\n', '\r'];
            let yaml = yaml.trim_start_matches(p);
            self.yaml = TextModel::create(yaml, Some("yaml"), None).ok();
        } else {
            self.yaml = None;
        }
    }

    fn render_content(&self, ctx: &Context<Self>, device: &Device) -> Html {
        let app = device.metadata.application.clone();
        let name = device.metadata.name.clone();

        let mapper = Mapper::new_callback(
            |parent: AppRoute| match parent {
                AppRoute::Devices(Pages::Details { details, .. }) => Some(details),
                _ => None,
            },
            move |child: DetailsSection| {
                AppRoute::Devices(Pages::Details {
                    app: ApplicationContext::Single(app.clone()),
                    name: name.clone(),
                    details: child,
                })
            },
        );

        html! {
            <>
                <Scope<AppRoute,DetailsSection> {mapper}>
                    <PageSection variant={PageSectionVariant::Light}>
                        <TabsRouter<DetailsSection>>
                            <TabRouterItem<DetailsSection> to={DetailsSection::Overview} label="Overview"/>
                            <TabRouterItem<DetailsSection> to={DetailsSection::Yaml} label="YAML"/>
                            <TabRouterItem<DetailsSection> to={DetailsSection::Debug} label="Events"/>
                        </TabsRouter<DetailsSection>>
                    </PageSection>
                    <PageSection>
                    {
                        match ctx.props().details {
                            DetailsSection::Overview => self.render_overview(ctx, device),
                            DetailsSection::Yaml => self.render_editor(ctx),
                            DetailsSection::Debug => self.render_debug(ctx),
                        }
                    }
                    </PageSection>
                </Scope<AppRoute,DetailsSection>>
            </>
        }
    }

    fn render_overview(&self, ctx: &Context<Self>, device: &Device) -> Html {
        let app = device.metadata.application.clone();
        html! {
            <Grid gutter=true>
                <GridItem cols={[3]}>
                    <Card
                        title={html_prop!({"Details"})}
                    >
                    <DescriptionList>
                        <DescriptionGroup term="Application">
                            <a onclick={ctx.link().callback(move |_| Msg::ShowApp(app.clone()))}>
                                {&device.metadata.application}</a>
                        </DescriptionGroup>
                        <DescriptionGroup term="Name">
                            {&device.metadata.name}
                        </DescriptionGroup>
                        <DescriptionGroup term="Labels">
                            { for device.metadata.labels.iter().map(|(k,v)|
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
            </Grid>
        }
    }

    fn render_editor(&self, ctx: &Context<Self>) -> Html {
        let options = CodeEditorOptions::default()
            .with_scroll_beyond_last_line(false)
            .with_language("yaml".to_owned())
            .with_builtin_theme(BuiltinTheme::VsDark);

        html! {
            <>
            <Stack>
                <StackItem fill=true>
                    <CodeEditor model={self.yaml.clone()} options={options.to_sys_options()} />
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
        }
    }

    fn render_debug(&self, ctx: &Context<Self>) -> Html {
        html! (
            <debug::Debug
                backend={ctx.props().backend.deref().clone()}
                application={ctx.props().app.clone()}
                endpoints={ctx.props().endpoints.clone()}
                device={ctx.props().name.clone()}
                />
        )
    }
}
