use crate::{backend::Backend, error::error};
use drogue_cloud_service_api::api::{ApiKey, ApiKeyCreated};
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*};

#[derive(Clone, Debug, PartialEq)]
pub struct ApiKeyEntry {
    pub key: ApiKey,
    pub on_delete: Callback<()>,
}

impl TableRenderer for ApiKeyEntry {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => self.key.prefix.clone().into(),
            1 => self.key.created.to_string().into(),
            2 => self
                .key
                .description
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "".to_string())
                .into(),
            _ => html! {},
        }
    }

    fn actions(&self) -> Vec<DropdownChildVariant> {
        vec![html_nested! {
        <DropdownItem
            onclick=self.on_delete.clone()
        >
            {"Delete"}
        </DropdownItem>}
        .into()]
    }
}

#[derive(Clone, PartialEq, Eq, Properties)]
pub struct Props {
    pub backend: Backend,
}

pub enum Msg {
    Create,
    Created(ApiKeyCreated),
    Load,
    SetData(Vec<ApiKeyEntry>),
    Delete(ApiKey),
    Deleted,
    Error(String),
}

pub struct ApiKeys {
    props: Props,
    link: ComponentLink<Self>,
    keys: Vec<ApiKeyEntry>,

    fetch_task: Option<FetchTask>,
}

impl Component for ApiKeys {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::Load);
        Self {
            props,
            link,
            keys: Vec::new(),
            fetch_task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Load => match self.load() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::SetData(keys) => {
                self.keys = keys;
                self.fetch_task = None;
            }
            Msg::Error(msg) => {
                error("Error", msg);
            }
            Msg::Delete(key) => match self.delete(key) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to delete key", err),
            },
            Msg::Deleted => {
                self.fetch_task = None;
                ToastDispatcher::default().toast(Toast {
                    title: "Deleted access key".into(),
                    body: html! {<p>{"Access key was successfully deleted."}</p>},
                    r#type: Type::Success,
                    ..Default::default()
                });
                self.link.send_message(Msg::Load);
            }
            Msg::Create => match self.create() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to create key", err),
            },
            Msg::Created(key) => {
                self.fetch_task = None;
                ToastDispatcher::default().toast(Toast {
                    title: "Created access key".into(),
                    body: html!{<>
                        <Content>
                        <p>{"A new access key was successfully created. The access key is:"}</p>
                        <p>
                        <Clipboard
                            value=key.key
                            readonly=true
                            />
                        </p>
                        <p>{"When you close this alert, you won't have any chance to get the access key ever again. Be sure to copy is somewhere safe."}</p>
                        </Content>
                    </>},
                    r#type: Type::Success,
                    ..Default::default()
                });
                self.link.send_message(Msg::Load);
            }
        };
        true
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        true
    }

    fn view(&self) -> Html {
        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <Title>{"Access keys"}</Title>
                    </Content>
                </PageSection>
                <PageSection>
                    <Toolbar>
                        <ToolbarGroup
                            modifiers=vec![ToolbarElementModifier::Right.all()]
                            >
                            <ToolbarItem>
                                <Button
                                    label="Create key"
                                    variant=Variant::Primary
                                    onclick=self.link.callback(|_|Msg::Create)
                                />
                            </ToolbarItem>
                        </ToolbarGroup>
                    </Toolbar>
                    <Table<SimpleTableModel<ApiKeyEntry>>
                        entries=SimpleTableModel::from(self.keys.clone())
                        header={html_nested!{
                            <TableHeader>
                                <TableColumn label="Prefix"/>
                                <TableColumn label="Timestamp (UTC)"/>
                                <TableColumn label="Description"/>
                            </TableHeader>
                        }}
                        >
                    </Table<SimpleTableModel<ApiKeyEntry>>>
                </PageSection>
            </>
        };
    }
}

impl ApiKeys {
    fn load(&self) -> Result<FetchTask, anyhow::Error> {
        let link = self.link.clone();

        self.props.backend.info.request(
            Method::GET,
            "/api/keys/v1alpha1",
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<Vec<ApiKey>, anyhow::Error>>>| match response
                    .into_body()
                    .0
                {
                    Ok(keys) => {
                        let link = link.clone();
                        let keys = keys
                            .into_iter()
                            .map(move |key| ApiKeyEntry {
                                key: key.clone(),
                                on_delete: link.clone().callback_once(|_| Msg::Delete(key)),
                            })
                            .collect();
                        Msg::SetData(keys)
                    }
                    Err(err) => Msg::Error(err.to_string()),
                },
            ),
        )
    }

    fn delete(&self, key: ApiKey) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::DELETE,
            format!("/api/keys/v1alpha1/{}", key.prefix),
            Nothing,
            vec![],
            self.link.callback(move |response: Response<Text>| {
                if response.status().is_success() {
                    Msg::Deleted
                } else {
                    Msg::Error(format!(
                        "Failed deleting API key: {}",
                        response.body().as_ref().map(|s| s.as_str()).unwrap_or("")
                    ))
                }
            }),
        )
    }

    fn create(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::POST,
            "/api/keys/v1alpha1",
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<ApiKeyCreated, anyhow::Error>>>| match response
                    .into_body()
                    .0
                {
                    Ok(key) => Msg::Created(key),
                    Err(err) => Msg::Error(format!("Failed creating API key: {}", err)),
                },
            ),
        )
    }
}
