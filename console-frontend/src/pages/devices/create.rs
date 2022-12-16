use crate::utils::context::ContextListener;
use crate::{
    backend::{ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, RequestHandle},
    error::{error, ErrorNotification, ErrorNotifier},
    pages::devices::{AppRoute, ApplicationContext, DetailsSection, Pages},
    utils::{success, url_encode},
};
use http::{Method, StatusCode};
use patternfly_yew::*;
use serde_json::json;
use yew::prelude::*;
use yew_nested_router::prelude::*;

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
    pub on_close: Callback<()>,
    pub app: String,
}

pub enum Msg {
    Success,
    Error(ErrorNotification),
    Create,
    NewDeviceName(String),
}

pub struct CreateDialog {
    fetch_task: Option<RequestHandle>,

    backdropper: ContextListener<Backdropper>,
    toaster: ContextListener<Toaster>,
    router: ContextListener<RouterContext<AppRoute>>,

    new_device_name: String,
}

impl Component for CreateDialog {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            fetch_task: None,
            backdropper: ContextListener::expect(ctx, "Backdropper"),
            toaster: ContextListener::expect(ctx, "Toaster"),
            router: ContextListener::expect(ctx, "Router"),
            new_device_name: Default::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error(msg) => {
                self.backdropper.get().close();
                msg.toast(&self.toaster.get());
            }
            Msg::Create => {
                match self.create(ctx, self.new_device_name.clone(), ctx.props().app.clone()) {
                    Ok(task) => self.fetch_task = Some(task),
                    Err(err) => error(&self.toaster.get(), "Failed to create", err),
                }
            }
            Msg::Success => {
                ctx.props().on_close.emit(());
                self.backdropper.get().close();
                success(&self.toaster.get(), "Device successfully created");
                self.router.get().push(AppRoute::Devices(Pages::Details {
                    app: ApplicationContext::Single(ctx.props().app.clone()),
                    name: self.new_device_name.clone(),
                    details: DetailsSection::Overview,
                }));
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
                    title="Create a new device"
                    variant={ModalVariant::Small}
                    footer={html!(
                        <Button
                            variant={Variant::Primary}
                            disabled={!is_valid || self.fetch_task.is_some()}
                            r#type={ButtonType::Submit}
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
                                onchange={ctx.link().callback(Msg::NewDeviceName)}
                                placeholder="Device ID" />
                        </FormGroup>
                    </Form>
                </Modal>
            </Bullseye>
        )
    }
}

impl CreateDialog {
    fn create(
        &self,
        ctx: &Context<Self>,
        name: String,
        app: String,
    ) -> Result<RequestHandle, anyhow::Error> {
        let payload = json!({
        "metadata": {
            "name": name,
            "application": app
        },
        "spec": {},
        });

        Ok(ctx.props().backend.request(
            Method::POST,
            format!("/api/registry/v1alpha1/apps/{}/devices", url_encode(app)),
            vec![],
            Json(&payload),
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::CREATED) => Msg::Success,
                response => Msg::Error(response.notify("Failed to create")),
            }),
        )?)
    }
}
