use crate::{
    backend::{ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, RequestHandle},
    error::{error, ErrorNotification, ErrorNotifier},
    pages::devices::{AppRoute, ApplicationContext, DetailsSection, Pages},
    utils::{success, url_encode},
};
use drogue_client::registry::v1::Device;
use http::{Method, StatusCode};
use patternfly_yew::*;
use yew::prelude::*;
use yew_router::{agent::RouteRequest, prelude::*};

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub on_close: Callback<()>,
    pub app: String,
    pub data: Device,
}

pub enum Msg {
    Success,
    Error(ErrorNotification),
    Create,
    NewDeviceName(String),
}

pub struct CloneDialog {
    new_device_name: String,
    fetch_task: Option<RequestHandle>,
}

impl Component for CloneDialog {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {
            fetch_task: None,
            new_device_name: Default::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error(msg) => {
                BackdropDispatcher::default().close();
                msg.toast();
            }
            Msg::Create => {
                match self.create(ctx, self.new_device_name.clone(), ctx.props().app.clone()) {
                    Ok(task) => self.fetch_task = Some(task),
                    Err(err) => error("Failed to create", err),
                }
            }
            Msg::Success => {
                ctx.props().on_close.emit(());
                BackdropDispatcher::default().close();
                success("Device cloned");
                RouteAgentDispatcher::<()>::new().send(RouteRequest::ChangeRoute(Route::from(
                    AppRoute::Devices(Pages::Details {
                        app: ApplicationContext::Single(ctx.props().app.clone()),
                        name: self.new_device_name.clone(),
                        details: DetailsSection::Overview,
                    }),
                )))
            }
            Msg::NewDeviceName(name) => self.new_device_name = name,
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let is_valid = matches!(self.new_device_name.len(), 1..=255);
        let v = |ctx: ValidationContext<String>| match ctx.value.len() {
            1..=255 => InputState::Default,
            _ => InputState::Error,
        };

        html! (
            <Bullseye plain=true>
                <Modal
                    title="New device name"
                    variant={ModalVariant::Small}
                    footer={html!(
                        <Button
                            variant={Variant::Primary}
                            disabled={!is_valid || self.fetch_task.is_some()}
                            r#type="submit"
                            onclick={ctx.link().callback(|_|Msg::Create)}
                            form="create-form"
                        >
                            {"Clone"}
                        </Button>
                    )}
                >
                    <Form id="create-form" method="dialog">
                        <FormGroup>
                            <TextInput
                                autofocus=true
                                validator={Validator::from(v)}
                                onchange={ctx.link().callback(Msg::NewDeviceName)}
                                placeholder="Device ID" />
                        </FormGroup>
                    </Form>
                </Modal>
            </Bullseye>
        )
    }
}

impl CloneDialog {
    fn create(
        &self,
        ctx: &Context<Self>,
        name: String,
        app: String,
    ) -> Result<RequestHandle, anyhow::Error> {
        let mut data = ctx.props().data.clone();
        data.metadata.name = name;

        Ok(ctx.props().backend.request(
            Method::POST,
            format!("/api/registry/v1alpha1/apps/{}/devices", url_encode(app)),
            vec![],
            Json(data),
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::CREATED) => Msg::Success,
                response => Msg::Error(response.notify("Failed to create")),
            }),
        )?)
    }
}
