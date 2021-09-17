mod admin;
mod integrations;

use super::{ApplicationTabs, Pages};
use crate::pages::apps::details::admin::{Admin, RoleEntry};
use crate::{
    backend::{Backend, Token},
    error::error,
    page::AppRoute,
    pages::{
        apps::{details::integrations::IntegrationDetails, DetailsSection},
        HasReadyState,
    },
    utils::{to_yaml_model, url_encode},
};
use drogue_client::registry::v1::Application;
use drogue_cloud_admin_service::apps::Members;
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

pub enum Msg {
    Load,
    LoadMembers,
    Reset,
    SetData(Application),
    SetMembers(Members),
    Error(String),
    SaveEditor,
}

pub struct Details {
    props: Props,
    link: ComponentLink<Self>,

    fetch_task: Option<FetchTask>,

    content: Option<Application>,
    yaml: Option<TextModel>,
    members: Option<Members>,
}

impl Component for Details {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::Load);
        link.send_message(Msg::LoadMembers);

        Self {
            props,
            link,
            content: None,
            yaml: None,
            fetch_task: None,
            members: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Load => match self.load() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to load", err),
            },
            Msg::LoadMembers => match self.load_permisisons() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to load", err),
            },
            Msg::SetData(content) => {
                self.content = Some(content);
                self.reset();
                self.fetch_task = None;
            }
            Msg::SetMembers(content) => {
                self.members = Some(content);
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
                error("Error", msg);
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
    fn load(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::GET,
            format!(
                "/api/registry/v1alpha1/apps/{}",
                url_encode(&self.props.name)
            ),
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<Application, anyhow::Error>>>| match response
                    .into_body()
                    .0
                {
                    Ok(content) => Msg::SetData(content),
                    Err(err) => Msg::Error(err.to_string()),
                },
            ),
        )
    }

    fn load_permisisons(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::GET,
            format!(
                "/api/admin/v1alpha1/apps/{}/members",
                url_encode(&self.props.name)
            ),
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<Members, anyhow::Error>>>| match response
                    .into_body()
                    .0
                {
                    Ok(content) => Msg::SetRoles(content),
                    Err(err) => Msg::Error(err.to_string()),
                },
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
                    status => Msg::Error(format!("Failed to perform update: {}", status)),
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
            self.yaml = to_yaml_model(content).ok();
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

        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light>
                    <ApplicationTabs
                        transformer=transformer
                        >
                        <TabRouterItem<DetailsSection> to=DetailsSection::Overview label="Overview"/>
                        <TabRouterItem<DetailsSection> to=DetailsSection::Integrations label="Integrations"/>
                        <TabRouterItem<DetailsSection> to=DetailsSection::Yaml label="YAML"/>
                        <TabRouterItem<DetailsSection> to=DetailsSection::Administration label="Administration"/>
                    </ApplicationTabs>
                </PageSection>
                <PageSection>
                {
                    match self.props.details {
                        DetailsSection::Overview => self.render_overview(app),
                        DetailsSection::Integrations => self.render_integrations(app),
                        DetailsSection::Yaml => self.render_editor(),
                        DetailsSection::Administration => self.render_admin(app),
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

    fn render_admin(&self, app: &Application) -> Html {
        Admin::from(self.members.unwrap()).render()
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
