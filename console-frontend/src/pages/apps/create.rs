use crate::error::error;
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::pages::apps::{AppRoute, DetailsSection, Pages};
use crate::utils::success;
use http::Method;
use patternfly_yew::*;
use yew::prelude::*;
use yew_router::{agent::RouteRequest, prelude::*};

use crate::backend::{ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, RequestHandle};
use serde_json::json;

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub on_close: Callback<()>,
}

pub enum Msg {
    Success,
    Error(ErrorNotification),
    Create,
    NewAppName(String),
}

pub struct CreateDialog {
    new_app_name: String,

    fetch_task: Option<RequestHandle>,
}

impl Component for CreateDialog {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {
            fetch_task: None,
            new_app_name: Default::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error(msg) => {
                BackdropDispatcher::default().close();
                msg.toast();
            }
            Msg::Create => match self.create(ctx, self.new_app_name.clone()) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to create", err),
            },
            Msg::Success => {
                ctx.props().on_close.emit(());
                BackdropDispatcher::default().close();
                success("Application successfully created");
                RouteAgentDispatcher::<()>::new().send(RouteRequest::ChangeRoute(Route::from(
                    AppRoute::Applications(Pages::Details {
                        name: self.new_app_name.clone(),
                        details: DetailsSection::Overview,
                    }),
                )))
            }
            Msg::NewAppName(name) => self.new_app_name = name,
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let is_valid = hostname_validator::is_valid(self.new_app_name.as_str());
        let v =
            |ctx: ValidationContext<String>| match hostname_validator::is_valid(ctx.value.as_str())
            {
                false => InputState::Error,
                true => InputState::Default,
            };

        html! (
            <Bullseye plain=true>
                <Modal
                    title={"Create an application"}
                    variant={ModalVariant::Small}
                    footer={html!(
                        <Button
                            variant={Variant::Primary}
                            disabled={!is_valid || self.fetch_task.is_some()}
                            r#type="submit"
                            onclick={ctx.link().callback(|_|Msg::Create)}
                            form="create-form"
                        >
                            {"Create"}
                        </Button>
                    )}
                >
                    <Form id="create-form" method="dialog">
                       <FormGroup>
                            <TextInput
                                autofocus=true
                                validator={Validator::from(v)}
                                onchange={ctx.link().callback(Msg::NewAppName)}
                                placeholder="Application ID"/>
                        </FormGroup>
                    </Form>
                </Modal>
            </Bullseye>
        )
    }
}

impl CreateDialog {
    fn create(&self, ctx: &Context<Self>, name: String) -> Result<RequestHandle, anyhow::Error> {
        let payload = json!({
        "metadata": {
            "name": name,
        },
        "spec": {},
        });

        Ok(ctx.props().backend.request(
            Method::POST,
            "/api/registry/v1alpha1/apps",
            vec![],
            Json(payload),
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, _) => Msg::Success,
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to create")),
            }),
        )?)
    }
}
