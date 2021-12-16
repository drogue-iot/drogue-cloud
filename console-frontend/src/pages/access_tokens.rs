use crate::backend::{ApiResponse, Json, JsonHandlerScopeExt, Nothing, RequestHandle};
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::{backend::Backend, error::error};
use drogue_cloud_service_api::token::{AccessToken, AccessTokenCreated};
use http::Method;
use patternfly_yew::*;
use yew::prelude::*;

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
            onclick={self.on_delete.clone()}
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
    tokens: Vec<AccessTokenEntry>,

    fetch_task: Option<RequestHandle>,
}

impl Component for AccessTokens {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::Load);
        Self {
            tokens: Vec::new(),
            fetch_task: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => match self.load(ctx) {
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
            Msg::Delete(token) => match self.delete(ctx, token) {
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
                ctx.link().send_message(Msg::Load);
            }
            Msg::Create => match self.create(ctx) {
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
                            value={token.token}
                            readonly=true
                            name="access-token"
                            />
                        </p>
                        <p>{"Once you close this alert, you won't have any chance to get the access token ever again. Be sure to copy it somewhere safe."}</p>
                        </Content>
                    </>},
                    r#type: Type::Success,
                    ..Default::default()
                });
                ctx.link().send_message(Msg::Load);
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let header = html_nested! {
            <TableHeader>
                <TableColumn label="Prefix"/>
                <TableColumn label="Timestamp (UTC)"/>
                <TableColumn label="Description"/>
            </TableHeader>
        };
        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Content>
                        <Title>{"Access tokens"}</Title>
                    </Content>
                </PageSection>
                <PageSection>
                    <Toolbar>
                        <ToolbarGroup
                            modifiers={[ToolbarElementModifier::Right.all()]}
                            >
                            <ToolbarItem>
                                <Button
                                    label="Create token"
                                    variant={Variant::Primary}
                                    onclick={ctx.link().callback(|_|Msg::Create)}
                                    id="create-token"
                                />
                            </ToolbarItem>
                        </ToolbarGroup>
                    </Toolbar>
                    <Table<SharedTableModel<AccessTokenEntry>>
                        entries={SharedTableModel::from(self.tokens.clone())}
                        header={header}
                        >
                    </Table<SharedTableModel<AccessTokenEntry>>>
                </PageSection>
            </>
        };
    }
}

impl AccessTokens {
    fn load(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        let link = ctx.link().clone();

        Ok(ctx.props().backend.info.request(
            Method::GET,
            "/api/tokens/v1alpha1",
            Nothing,
            vec![],
            ctx.callback_api::<Json<Vec<AccessToken>>, _>(move |response| match response {
                ApiResponse::Success(tokens, _) => {
                    let link = link.clone();
                    let tokens = tokens
                        .into_iter()
                        .map(move |token| AccessTokenEntry {
                            token: token.clone(),
                            on_delete: link.clone().callback_once(|_| Msg::Delete(token)),
                        })
                        .collect();
                    Msg::SetData(tokens)
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to load")),
            }),
        )?)
    }

    fn delete(
        &self,
        ctx: &Context<Self>,
        token: AccessToken,
    ) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.info.request(
            Method::DELETE,
            format!("/api/tokens/v1alpha1/{}", token.prefix),
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, _) => Msg::Deleted,
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to delete")),
            }),
        )?)
    }

    fn create(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.info.request(
            Method::POST,
            "/api/tokens/v1alpha1",
            Nothing,
            vec![],
            ctx.callback_api::<Json<AccessTokenCreated>, _>(move |response| match response {
                ApiResponse::Success(token, _) => Msg::Created(token),
                ApiResponse::Failure(err) => Msg::Error(err.notify("Creation failed")),
            }),
        )?)
    }
}
