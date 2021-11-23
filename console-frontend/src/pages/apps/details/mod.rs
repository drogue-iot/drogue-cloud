mod admin;
mod debug;
mod integrations;

use super::{ApplicationTabs, Pages};
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::JsonResponse;
use crate::{
    backend::{Backend, Token},
    error::error,
    page::AppRoute,
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
use drogue_cloud_service_api::kafka::{KafkaConfigExt, KafkaEventType, KafkaTarget};
use monaco::{api::*, sys::editor::BuiltinTheme, yew::CodeEditor};
use patternfly_yew::*;
use std::rc::Rc;
use yew::{format::*, prelude::*, services::fetch::*};

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
    props: Props,
    link: ComponentLink<Self>,

    fetch_task: Option<FetchTask>,
    fetch_role: Option<FetchTask>,

    content: Option<Rc<Application>>,
    yaml: Option<TextModel>,
    is_admin: bool,
}

impl Component for Details {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::Load);

        Self {
            props,
            link,
            content: None,
            yaml: None,
            fetch_task: None,
            fetch_role: None,
            is_admin: false,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Load => match self.load() {
                (Ok(task), Ok(admin_task)) => {
                    self.fetch_task = Some(task);
                    self.fetch_role = Some(admin_task);
                }
                (Err(err), _) | (_, Err(err)) => error("Failed to load", err),
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
                    match self.update_yaml(&new_content) {
                        Ok(task) => self.fetch_task = Some(task),
                        Err(err) => error("Failed to update", err),
                    }
                }
            }
            Msg::Error(msg) => {
                msg.toast();
            }
            Msg::SetAdmin(is_admin) => {
                self.fetch_role = None;
                self.is_admin = is_admin;
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
                        <Title>{&self.props.name}</Title>
                    </Content>
                </PageSection>
            { if let Some(app) = &self.content {
                self.render_content(app)
            } else {
                html!{<PageSection><Grid></Grid></PageSection>}
            } }
            </>
        };
    }
}

impl Details {
    fn load(
        &self,
    ) -> (
        Result<FetchTask, anyhow::Error>,
        Result<FetchTask, anyhow::Error>,
    ) {
        (
            self.props.backend.info.request(
                Method::GET,
                format!(
                    "/api/registry/v1alpha1/apps/{}",
                    url_encode(&self.props.name)
                ),
                Nothing,
                vec![],
                self.link
                    .callback(move |response: JsonResponse<Application>| {
                        match response.into_body().0 {
                            Ok(content) => Msg::SetData(Rc::new(content.value)),
                            Err(err) => Msg::Error(err.notify("Failed to load")),
                        }
                    }),
            ),
            self.props.backend.info.request(
                Method::GET,
                format!(
                    "/api/admin/v1alpha1/apps/{}/members",
                    url_encode(&self.props.name)
                ),
                Nothing,
                vec![],
                self.link
                    .callback(move |response: Response<Text>| match response.status() {
                        status if status.is_success() => Msg::SetAdmin(true),
                        _ => Msg::SetAdmin(false),
                    }),
            ),
        )
    }

    fn update(&self, app: Application) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::PUT,
            format!(
                "/api/registry/v1alpha1/apps/{}",
                url_encode(&self.props.name)
            ),
            Json(&app),
            vec![("Content-Type", "application/json")],
            self.link
                .callback(move |response: Response<Text>| match response.status() {
                    status if status.is_success() => Msg::Load,
                    _ => Msg::Error(response.notify("Failed to update")),
                }),
        )
    }

    fn update_yaml(&self, yaml: &str) -> Result<FetchTask, anyhow::Error> {
        let app = serde_yaml::from_str(yaml)?;
        log::info!("Updating to: {:#?}", app);
        self.update(app)
    }

    fn reset(&mut self) {
        if let Some(content) = &self.content {
            self.yaml = to_yaml_model(content.as_ref()).ok();
        } else {
            self.yaml = None;
        }
    }

    fn render_content(&self, app: &Application) -> Html {
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
           <TabRouterItem<DetailsSection> to=DetailsSection::Overview label="Overview"/>
        });
        tabs.push(html_nested! {
           <TabRouterItem<DetailsSection> to=DetailsSection::Integrations label="Integrations"/>
        });
        tabs.push(html_nested! {
           <TabRouterItem<DetailsSection> to=DetailsSection::Yaml label="YAML"/>
        });
        tabs.push(html_nested! {
           <TabRouterItem<DetailsSection> to=DetailsSection::Debug label="Debug"/>
        });
        if self.is_admin && self.fetch_role.is_none() {
            tabs.push(html_nested!{
           <TabRouterItem<DetailsSection> to=DetailsSection::Administration label="Administration"/>
        });
        }

        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light>
                    <ApplicationTabs
                        transformer=transformer
                        >
                        { tabs }
                    </ApplicationTabs>
                </PageSection>
                <PageSection>
                {
                    match self.props.details {
                        DetailsSection::Overview => self.render_overview(app),
                        DetailsSection::Integrations => self.render_integrations(app),
                        DetailsSection::Yaml => self.render_editor(),
                        DetailsSection::Debug => self.render_debug(app),
                        DetailsSection::Administration => self.render_admin(),
                    }
                }
                </PageSection>
            </>
        };
    }

    fn render_overview(&self, app: &Application) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols=[3]>
                    <Card
                        title={html_nested!{<>{"Details"}</>}}
                        >
                        <DescriptionList>
                            <DescriptionGroup term="Name">
                                {&app.metadata.name}
                            </DescriptionGroup>
                            <DescriptionGroup term="Labels">
                                { for app.metadata.labels.iter().map(|(k,v)|
                                    if v.is_empty() {
                                        html!{ <Label label=k.clone()/>}
                                    } else {
                                        html!{ <Label label=format!("{}={}", k, v)/>}
                                    }
                                ) }
                            </DescriptionGroup>
                        </DescriptionList>
                    </Card>
                </GridItem>
                <GridItem cols=[3]>
                    <Card
                        title={html_nested!{<>{"Kafka"}</>}}
                        >
                        <DescriptionList>
                            <DescriptionGroup term="State">
                                {app.render_condition("KafkaReady")}
                            </DescriptionGroup>
                            <DescriptionGroup term="Type">
                            {
                                match app.kafka_target(KafkaEventType::Events) {
                                    Ok(KafkaTarget::Internal{ topic }) => html!{
                                        {"Internal "}
                                    },
                                    Ok(KafkaTarget::External{..}) => html!{
                                        {"External"}
                                    },
                                    Err(err) => {
                                        log::info!("Failed to eval kafka target: {}", err);
                                        html!{}
                                    },
                                }
                            }
                            </DescriptionGroup>
                        </DescriptionList>
                    </Card>
                </GridItem>
            </Grid>
        };
    }

    fn render_admin(&self) -> Html {
        // create the Admin component, using a copy of the same props.
        return html! {
            <Admin
                backend={self.props.backend.clone()}
                token={self.props.token.clone()}
                endpoints={self.props.endpoints.clone()}
                name={self.props.name.clone()}
                details={self.props.details.clone()}
            />
        };
    }

    fn render_integrations(&self, application: &Application) -> Html {
        IntegrationDetails {
            backend: &self.props.backend,
            application,
            token: &self.props.token,
            endpoints: &self.props.endpoints,
        }
        .render()
    }

    fn render_debug(&self, application: &Application) -> Html {
        return html! {
            <debug::Debug
                backend=self.props.backend.clone()
                application=application.metadata.name.clone()
                endpoints=self.props.endpoints.clone()
                token=self.props.token.clone()
                />
        };
    }

    fn render_editor(&self) -> Html {
        let options = CodeEditorOptions::default()
            .with_scroll_beyond_last_line(false)
            .with_language("yaml".to_owned())
            .with_builtin_theme(BuiltinTheme::VsDark);

        let options = Rc::new(options);

        return html! {
            <>
            <Stack>
                <StackItem fill=true>
                    <CodeEditor model=self.yaml.clone() options=options/>
                </StackItem>
                <StackItem>
                    <Form>
                    <ActionGroup>
                        <Button disabled=self.fetch_task.is_some() label="Save" variant=Variant::Primary onclick=self.link.callback(|_|Msg::SaveEditor)/>
                        <Button disabled=self.fetch_task.is_some() label="Reload" variant=Variant::Secondary onclick=self.link.callback(|_|Msg::Load)/>
                        <Button disabled=self.fetch_task.is_some() label="Cancel" variant=Variant::Secondary onclick=self.link.callback(|_|Msg::Reset)/>
                    </ActionGroup>
                    </Form>
                </StackItem>
            </Stack>
            </>
        };
    }
}
