use crate::backend::{
    ApiResponse, AuthenticatedBackend, JsonHandlerScopeExt, Nothing, RequestHandle,
};
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::{success, url_encode};
use crate::{
    console::AppRoute,
    error::error,
    html_prop,
    pages::apps::{DetailsSection, Pages},
};
use gloo_timers::callback::Timeout;
use http::{Method, StatusCode};
use patternfly_yew::*;
use yew::prelude::*;
use yew_router::{agent::RouteRequest, prelude::*};

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub name: String,
}

pub enum Msg {
    Accept,
    Error(ErrorNotification),
    Success,
    Done,
    Decline,
    TransferPending(bool),
    Load,
}

pub struct Ownership {
    fetch_task: Option<RequestHandle>,
    timeout: Option<Timeout>,

    transfer_active: bool,
}

impl Component for Ownership {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::Load);

        Self {
            fetch_task: None,
            timeout: None,
            transfer_active: false,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => match self.load(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to load transfer state", err),
            },
            Msg::Accept => match self.accept(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::Decline => match self.cancel(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to cancel", err),
            },
            Msg::Error(msg) => {
                msg.toast();
            }
            Msg::Done => RouteAgentDispatcher::<()>::new().send(RouteRequest::ChangeRoute(
                Route::from(AppRoute::Applications(Pages::Details {
                    name: ctx.props().name.clone(),
                    details: DetailsSection::Overview,
                })),
            )),
            Msg::TransferPending(pending) => {
                self.fetch_task = None;
                self.transfer_active = pending;
                if !pending {
                    error(
                        "Transfer unavailable",
                        "This application transfer is not active. Maybe it was cancelled",
                    );
                }
            }
            Msg::Success => {
                success("Ownership transfer completed. You are now the owner of this application.");

                // Set a timeout before leaving the page.
                let link = ctx.link().clone();
                let handle = Timeout::new(3_000, move || {
                    link.send_message(Msg::Done);
                });

                // Keep the task or timer will be cancelled
                self.timeout = Some(handle);
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Card title={html_prop!({"Application ownership transfer"})}>
                        <p>{format!("Application name: {}", &ctx.props().name)}</p>
                        <Toolbar>
                        <ToolbarGroup>
                            <ToolbarItem>
                                    <Button
                                            disabled={self.fetch_task.is_some() || !self.transfer_active}
                                            label="Accept"
                                            icon={Icon::CheckCircle}
                                            variant={Variant::Primary}
                                            onclick={ctx.link().callback(|_|Msg::Accept)}
                                    />
                                    <Button
                                            disabled={self.fetch_task.is_some() || !self.transfer_active}
                                            label="Decline"
                                            variant={Variant::Secondary}
                                            onclick={ctx.link().callback(|_|Msg::Decline)}
                                    />
                            </ToolbarItem>
                        </ToolbarGroup>
                        </Toolbar>
                    </Card>
                </PageSection>
            </>
        };
    }
}

impl Ownership {
    fn load(&mut self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::GET,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&ctx.props().name)
            ),
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::OK) => Msg::TransferPending(true),
                ApiResponse::Success(_, StatusCode::NO_CONTENT) => Msg::TransferPending(false),
                response => Msg::Error(response.notify("Failed to fetch transfer state")),
            }),
        )?)
    }

    fn accept(&mut self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::PUT,
            format!(
                "/api/admin/v1alpha1/apps/{}/accept-ownership",
                url_encode(&ctx.props().name)
            ),
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::NO_CONTENT) => Msg::Success,
                response => Msg::Error(response.notify("Failed to accept ownership")),
            }),
        )?)
    }

    fn cancel(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::DELETE,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&ctx.props().name)
            ),
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::NO_CONTENT) => Msg::TransferPending(false),
                response => Msg::Error(response.notify("Failed to cancel transfer")),
            }),
        )?)
    }
}
