mod create;
mod success;

use crate::backend::{
    ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, Nothing, RequestHandle,
};
use crate::error::{error, ErrorNotification, ErrorNotifier};
use crate::utils::context::ContextListener;
use create::AccessTokenCreateModal;
use drogue_cloud_service_api::token::AccessToken;
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

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
}

pub enum Msg {
    CreateModal,
    // Created(AccessTokenCreated),
    Load,
    SetData(Vec<AccessTokenEntry>),
    Delete(AccessToken),
    Deleted,
    Error(ErrorNotification),
}

pub struct AccessTokens {
    tokens: Vec<AccessTokenEntry>,

    fetch_task: Option<RequestHandle>,

    backdropper: ContextListener<Backdropper>,
    toaster: ContextListener<Toaster>,
}

impl Component for AccessTokens {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::Load);
        Self {
            tokens: Vec::new(),
            fetch_task: None,

            backdropper: ContextListener::unwrap(ctx),
            toaster: ContextListener::unwrap(ctx),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => match self.load(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error(&self.toaster.get(), "Failed to fetch", err),
            },
            Msg::SetData(tokens) => {
                self.tokens = tokens;
                self.fetch_task = None;
            }
            Msg::Error(msg) => {
                msg.toast(&self.toaster.get());
            }
            Msg::Delete(token) => match self.delete(ctx, token) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error(&self.toaster.get(), "Failed to delete token", err),
            },
            Msg::Deleted => {
                self.fetch_task = None;
                self.toaster.get().toast(Toast {
                    title: "Deleted access token".into(),
                    body: html! {<p>{"Access token was successfully deleted."}</p>},
                    r#type: Type::Success,
                    ..Default::default()
                });
                ctx.link().send_message(Msg::Load);
            }
            Msg::CreateModal => self.backdropper.get().open(html! {
                <AccessTokenCreateModal
                    backend={ctx.props().backend.clone()}
                    on_close={ctx.link().callback(move |_| Msg::Load)}
                />
            }),
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
                                    onclick={ctx.link().callback(|_|Msg::CreateModal)}
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

        Ok(ctx.props().backend.request(
            Method::GET,
            "/api/tokens/v1alpha1",
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<Json<Vec<AccessToken>>, _>(move |response| match response {
                ApiResponse::Success(tokens, _) => {
                    let tokens = tokens
                        .into_iter()
                        .map(move |token| AccessTokenEntry {
                            token: token.clone(),
                            on_delete: link.clone().callback(move |_| Msg::Delete(token.clone())),
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
        Ok(ctx.props().backend.request(
            Method::DELETE,
            format!("/api/tokens/v1alpha1/{}", token.prefix),
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, _) => Msg::Deleted,
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to delete")),
            }),
        )?)
    }
}
