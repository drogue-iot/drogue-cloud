use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::url_encode;
use crate::{backend::Backend, error::error};
use http::{Method, StatusCode};

use patternfly_yew::*;
use yew::prelude::*;

use crate::backend::{ApiResponse, Json, JsonHandlerScopeExt, RequestHandle};
use serde_json::json;

#[derive(Clone, PartialEq, Properties)]
pub struct Props {
    pub backend: Backend,
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
    new_device_name: String,

    fetch_task: Option<RequestHandle>,
}

impl Component for CreateDialog {
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
                BackdropDispatcher::default().close()
            }
            Msg::NewDeviceName(name) => self.new_device_name = name,
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let is_valid = matches!(self.new_device_name.len(), 1..=255);
        let v = |value: &str| match value.len() {
            1..=255 => InputState::Default,
            _ => InputState::Error,
        };

        return html! {
            <>
            <Bullseye plain=true>
            <Modal
                title = {"Create a new device"}
                variant= {ModalVariant::Small}
                footer = {{html!{
                            <>
                                <button class="pf-c-button pf-m-primary"
                                    disabled={!is_valid || self.fetch_task.is_some()}
                                    type="button"
                                    onclick={ctx.link().callback(|_|Msg::Create)}
                                >
                                    {"Create"}</button>
                            </>}
                            }}
            >
                <Form>
                       <FormGroup>
                            <TextInput
                                validator={Validator::from(v)}
                                onchange={ctx.link().callback(|id|Msg::NewDeviceName(id))}
                                placeholder="Device ID"/>
                        </FormGroup>
                </Form>
            </Modal>
            </Bullseye>
            </>
        };
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

        Ok(ctx.props().backend.info.request(
            Method::POST,
            format!("/api/registry/v1alpha1/apps/{}/devices", url_encode(app)),
            Json(&payload),
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::CREATED) => Msg::Success,
                response => Msg::Error(response.notify("Failed to create")),
            }),
        )?)
    }
}
