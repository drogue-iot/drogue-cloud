use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::JsonResponse;
use crate::{backend::Backend, error::error};
use drogue_cloud_service_api::token::{AccessToken, AccessTokenCreated};
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*};

#[derive(Clone, Debug, PartialEq)]
pub struct AccessTokenEntry {
    pub token: AccessToken,
    pub on_delete: Callback<()>,
}

impl TableRenderer for AccessTokenEntry {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => self.token.prefix.clone().into(),
            1 => self.token.created.to_string().into(),
            2 => self
                .token
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
    Created(AccessTokenCreated),
    Load,
    SetData(Vec<AccessTokenEntry>),
    Delete(AccessToken),
    Deleted,
    Error(ErrorNotification),
}

pub struct AccessTokens {
    props: Props,
    link: ComponentLink<Self>,
    tokens: Vec<AccessTokenEntry>,

    fetch_task: Option<FetchTask>,
}

impl Component for AccessTokens {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::Load);
        Self {
            props,
            link,
            tokens: Vec::new(),
            fetch_task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Load => match self.load() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::SetData(tokens) => {
                self.tokens = tokens;
                self.fetch_task = None;
            }
            Msg::Error(msg) => {
                msg.toast();
            }
            Msg::Delete(token) => match self.delete(token) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to delete token", err),
            },
            Msg::Deleted => {
                self.fetch_task = None;
                ToastDispatcher::default().toast(Toast {
                    title: "Deleted access token".into(),
                    body: html! {<p>{"Access token was successfully deleted."}</p>},
                    r#type: Type::Success,
                    ..Default::default()
                });
                self.link.send_message(Msg::Load);
            }
            Msg::Create => match self.create() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to create token", err),
            },
            Msg::Created(token) => {
                self.fetch_task = None;
                ToastDispatcher::default().toast(Toast {
                    title: "Created access token".into(),
                    body: html!{<>
                        <Content>
                        <p>{"A new access token was successfully created. The access token is:"}</p>
                        <p>
                        <Clipboard
                            value=token.key
                            readonly=true
                            name="api-key"
                            />
                        </p>
                        <p>{"Once you close this alert, you won't have any chance to get the access token ever again. Be sure to copy it somewhere safe."}</p>
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
                        <Title>{"Access tokens"}</Title>
                    </Content>
                </PageSection>
                <PageSection>
                    <Toolbar>
                        <ToolbarGroup
                            modifiers=vec![ToolbarElementModifier::Right.all()]
                            >
                            <ToolbarItem>
                                <Button
                                    label="Create token"
                                    variant=Variant::Primary
                                    onclick=self.link.callback(|_|Msg::Create)
                                    id="create-key"
                                />
                            </ToolbarItem>
                        </ToolbarGroup>
                    </Toolbar>
                    <Table<SimpleTableModel<AccessTokenEntry>>
                        entries=SimpleTableModel::from(self.tokens.clone())
                        header={html_nested!{
                            <TableHeader>
                                <TableColumn label="Prefix"/>
                                <TableColumn label="Timestamp (UTC)"/>
                                <TableColumn label="Description"/>
                            </TableHeader>
                        }}
                        >
                    </Table<SimpleTableModel<AccessTokenEntry>>>
                </PageSection>
            </>
        };
    }
}

impl AccessTokens {
    fn load(&self) -> Result<FetchTask, anyhow::Error> {
        let link = self.link.clone();

        self.props.backend.info.request(
            Method::GET,
            "/api/tokens/v1alpha1",
            Nothing,
            vec![],
            self.link
                .callback(move |response: JsonResponse<Vec<AccessToken>>| {
                    match response.into_body().0 {
                        Ok(tokens) => {
                            let link = link.clone();
                            let tokens = tokens
                                .value
                                .into_iter()
                                .map(move |token| AccessTokenEntry {
                                    token: token.clone(),
                                    on_delete: link.clone().callback_once(|_| Msg::Delete(token)),
                                })
                                .collect();
                            Msg::SetData(tokens)
                        }
                        Err(err) => Msg::Error(err.notify("Failed to load")),
                    }
                }),
        )
    }

    fn delete(&self, token: AccessToken) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::DELETE,
            format!("/api/tokens/v1alpha1/{}", token.prefix),
            Nothing,
            vec![],
            self.link.callback(move |response: Response<Text>| {
                if response.status().is_success() {
                    Msg::Deleted
                } else {
                    Msg::Error(response.notify("Failed to delete"))
                }
            }),
        )
    }

    fn create(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::POST,
            "/api/tokens/v1alpha1",
            Nothing,
            vec![],
            self.link
                .callback(move |response: JsonResponse<AccessTokenCreated>| {
                    match response.into_body().0 {
                        Ok(token) => Msg::Created(token.value),
                        Err(err) => Msg::Error(err.notify("Creation failed")),
                    }
                }),
        )
    }
}
