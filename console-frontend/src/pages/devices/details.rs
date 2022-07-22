use super::{DevicesTabs, Pages};
use crate::backend::{
    ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, Nothing, RequestHandle,
};
use crate::{
    console::AppRoute,
    error::{error, ErrorNotification, ErrorNotifier},
    html_prop,
    pages::{apps::ApplicationContext, devices::DetailsSection},
    utils::url_encode,
};
use drogue_client::registry::v1::Device;
use http::{Method, StatusCode};
use monaco::{api::*, sys::editor::BuiltinTheme, yew::CodeEditor};
use patternfly_yew::*;
use std::rc::Rc;
use yew::prelude::*;
use yew_router::{agent::RouteRequest, prelude::*};

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub backend: AuthenticatedBackend,
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
    DeletionComplete,
}

pub struct Details {
    fetch_task: Option<RequestHandle>,

    content: Option<Rc<Device>>,
    yaml: Option<TextModel>,
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
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => match self.load(ctx) {
                Ok(task) => self.fetch_task = Some(task),
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
            Msg::Delete => match self.delete(ctx) {
                Ok(task) => {
                    self.fetch_task = Some(task);
                }
                Err(err) => error("Failed to delete", err),
            },
            Msg::DeletionComplete => RouteAgentDispatcher::<()>::new().send(
                RouteRequest::ChangeRoute(Route::from(AppRoute::Devices(Pages::Index {
                    app: ApplicationContext::Single(ctx.props().app.clone()),
                }))),
            ),
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Content>
                        <Flex>
                            <FlexItem>
                                <Title>{ctx.props().name.clone()}</Title>
                            </FlexItem>
                            <FlexItem modifiers={[FlexModifier::Align(Alignement::Right).all()]}>
                                <Button
                                        label="Delete"
                                        variant={Variant::DangerSecondary}
                                        onclick={ctx.link().callback(|_|Msg::Delete)}
                                />
                            </FlexItem>
                        </Flex>
                    </Content>
                </PageSection>
            { if let Some(app) = &self.content {
                self.render_content(ctx, app)
            } else {
                html!{<PageSection><Grid></Grid></PageSection>}
            } }
            </>
        };
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

    fn delete(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::DELETE,
            format!(
                "/api/registry/v1alpha1/apps/{}/devices/{}",
                url_encode(&ctx.props().app),
                url_encode(&ctx.props().name)
            ),
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::NO_CONTENT) => Msg::DeletionComplete,
                ApiResponse::Success(_, code) => {
                    Msg::Error(format!("Unknown message code: {}", code).notify("Failed to delete"))
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to delete")),
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
        let transformer = SwitchTransformer::new(
            |global| match global {
                AppRoute::Devices(Pages::Details { details, .. }) => Some(details),
                _ => None,
            },
            move |local| {
                AppRoute::Devices(Pages::Details {
                    app: ApplicationContext::Single(app.clone()),
                    name: name.clone(),
                    details: local,
                })
            },
        );

        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light}>
                    <DevicesTabs
                        transformer={transformer}
                        >
                        <TabRouterItem<DetailsSection> to={DetailsSection::Overview} label="Overview"/>
                        <TabRouterItem<DetailsSection> to={DetailsSection::Yaml} label="YAML"/>
                    </DevicesTabs>
                </PageSection>
                <PageSection>
                {
                    match ctx.props().details {
                        DetailsSection::Overview => self.render_overview(device),
                        DetailsSection::Yaml => self.render_editor(ctx),
                    }
                }
                </PageSection>
            </>
        };
    }

    fn render_overview(&self, device: &Device) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols={[3]}>
                    <Card
                        title={html_prop!({"Details"})}
                    >
                    <DescriptionList>
                        <DescriptionGroup term="Application">
                            {&device.metadata.application}
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
                    <CodeEditor model={self.yaml.clone()} options={options} />
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
