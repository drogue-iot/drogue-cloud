use super::{DevicesTabs, Pages};
use crate::{
    backend::Backend,
    error::error,
    page::AppRoute,
    pages::{apps::ApplicationContext, devices::DetailsSection},
    utils::url_encode,
};
use drogue_client::registry::v1::Device;
use monaco::{api::*, sys::editor::BuiltinTheme, yew::CodeEditor};
use patternfly_yew::*;
use std::rc::Rc;
use yew::{format::*, prelude::*, services::fetch::*};

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub backend: Backend,
    pub app: String,
    pub name: String,
    pub details: DetailsSection,
}

pub enum Msg {
    Load,
    Reset,
    SetData(Device),
    Error(String),
    SaveEditor,
}

pub struct Details {
    props: Props,
    link: ComponentLink<Self>,

    fetch_task: Option<FetchTask>,

    content: Option<Device>,
    yaml: Option<TextModel>,
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
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Load => match self.load() {
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
                "/api/registry/v1alpha1/apps/{}/devices/{}",
                url_encode(&self.props.app),
                url_encode(&self.props.name)
            ),
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<Device, anyhow::Error>>>| match response
                    .into_body()
                    .0
                {
                    Ok(content) => Msg::SetData(content),
                    Err(err) => Msg::Error(err.to_string()),
                },
            ),
        )
    }

    fn update(&self, app: Device) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::PUT,
            format!(
                "/api/registry/v1alpha1/apps/{}/devices/{}",
                url_encode(&self.props.app),
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
            let yaml = serde_yaml::to_string(content).unwrap_or_default();
            let p: &[_] = &['-', '\n', '\r'];
            let yaml = yaml.trim_start_matches(p);
            self.yaml = TextModel::create(yaml, Some("yaml"), None).ok();
        } else {
            self.yaml = None;
        }
    }

    fn render_content(&self, device: &Device) -> Html {
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
                <PageSection variant=PageSectionVariant::Light>
                    <DevicesTabs
                        transformer=transformer
                        >
                        <TabRouterItem<DetailsSection> to=DetailsSection::Overview label="Overview"/>
                        <TabRouterItem<DetailsSection> to=DetailsSection::Yaml label="YAML"/>
                    </DevicesTabs>
                </PageSection>
                <PageSection>
                {
                    match self.props.details {
                        DetailsSection::Overview => self.render_overview(device),
                        DetailsSection::Yaml => self.render_editor(),
                    }
                }
                </PageSection>
            </>
        };
    }

    fn render_overview(&self, device: &Device) -> Html {
        return html! {
            <Grid gutter=true>
                <GridItem cols=[3]>
                    <Card
                        title={html_nested!{<>{"Details"}</>}}
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
                                    html!{ <Label label=k.clone()/>}
                                } else {
                                    html!{ <Label label=format!("{}={}", k, v)/>}
                                }
                            ) }
                        </DescriptionGroup>
                    </DescriptionList>
                    </Card>
                </GridItem>
            </Grid>
        };
    }

    fn render_editor(&self) -> Html {
        let options = CodeEditorOptions::default()
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
