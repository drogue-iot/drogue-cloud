use crate::error::error;
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::pages::devices::Pages;
use crate::utils::{success, url_encode};
use http::{Method, StatusCode};
use patternfly_yew::*;
use yew::prelude::*;
use yew_nested_router::prelude::*;

use crate::backend::{
    ApiResponse, AuthenticatedBackend, JsonHandlerScopeExt, Nothing, RequestHandle,
};
use crate::console::AppRoute;
use crate::pages::apps::ApplicationContext;
use crate::utils::context::ContextListener;

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub on_close: Callback<()>,
    pub name: String,
    pub app_name: String,
}

pub enum Msg {
    Success,
    Error(ErrorNotification),
    Delete,
    Cancel,
}

pub struct DeleteConfirmation {
    fetch_task: Option<RequestHandle>,
    backdropper: ContextListener<Backdropper>,
    router: ContextListener<RouterContext<AppRoute>>,
}

impl Component for DeleteConfirmation {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            fetch_task: None,
            backdropper: ContextListener::new(ctx),
            router: ContextListener::new(ctx),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error(msg) => {
                self.backdropper.close();
                msg.toast();
            }
            Msg::Delete => match self.delete(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to Delete", err),
            },
            Msg::Success => {
                ctx.props().on_close.emit(());
                self.backdropper.close();
                success("Device deleted");
                self.router.go(AppRoute::Devices(Pages::Index {
                    app: ApplicationContext::Single(ctx.props().app_name.clone()),
                }));
            }
            Msg::Cancel => {
                ctx.props().on_close.emit(());
                self.backdropper.close();
            }
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! (
            <Bullseye plain=true>
                <Modal
                    title={format!("Delete device {} ?", ctx.props().name)}
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
                "/api/registry/v1alpha1/apps/{}/devices/{}",
                url_encode(&ctx.props().app_name),
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
