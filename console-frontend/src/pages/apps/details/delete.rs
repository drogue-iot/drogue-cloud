use crate::error::error;
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::pages::apps::{AppRoute, Pages};
use crate::utils::{success, url_encode};
use http::{Method, StatusCode};
use patternfly_yew::*;
use yew::prelude::*;
use yew_nested_router::{prelude::*};

use crate::backend::{
    ApiResponse, AuthenticatedBackend, JsonHandlerScopeExt, Nothing, RequestHandle,
};

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub on_close: Callback<()>,
    pub name: String,
}

pub enum Msg {
    Success,
    Error(ErrorNotification),
    Delete,
    Cancel,
}

pub struct DeleteConfirmation {
    fetch_task: Option<RequestHandle>,
}

impl Component for DeleteConfirmation {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self { fetch_task: None }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error(msg) => {
                use_backdrop().unwrap().close();
                msg.toast();
            }
            Msg::Delete => match self.delete(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to Delete", err),
            },
            Msg::Success => {
                ctx.props().on_close.emit(());
                use_backdrop().unwrap().close();
                success("Application deleted");
                use_router().unwrap().push(
                    AppRoute::Applications(Pages::Index),
                )
            }
            Msg::Cancel => {
                ctx.props().on_close.emit(());
                use_backdrop().unwrap().close();
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! (
            <Bullseye plain=true>
                <Modal
                    title={format!("Delete application {} ?", ctx.props().name)}
                    description={"This cannot be undone."}
                    variant={ModalVariant::Small}
                    footer={html!(
                        <>
                            <Button
                                label={"Delete"}
                                variant={Variant::Danger}
                                onclick={ctx.link().callback(|_|Msg::Delete)}
                            />
                            <Button
                                label={"Cancel"}
                                variant={Variant::Link}
                                onclick={ctx.link().callback(|_|Msg::Cancel)}
                            />
                        </>
                    )}
                >
                </Modal>
            </Bullseye>
        )
    }
}

impl DeleteConfirmation {
    fn delete(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::DELETE,
            format!(
                "/api/registry/v1alpha1/apps/{}",
                url_encode(&ctx.props().name)
            ),
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::NO_CONTENT) => Msg::Success,
                ApiResponse::Success(_, code) => {
                    Msg::Error(format!("Unknown message code: {}", code).notify("Failed to delete"))
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to delete")),
            }),
        )?)
    }
}
