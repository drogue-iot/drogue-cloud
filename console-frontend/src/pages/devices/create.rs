use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::url_encode;
use crate::{backend::Backend, error::error};

use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*};

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
    props: Props,
    link: ComponentLink<Self>,

    new_device_name: String,

    fetch_task: Option<FetchTask>,
}

impl Component for CreateDialog {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            props,
            link,
            fetch_task: None,
            new_device_name: Default::default(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Error(msg) => {
                BackdropDispatcher::default().close();
                msg.toast();
            }
            Msg::Create => {
                match self.create(self.new_device_name.clone(), self.props.app.clone()) {
                    Ok(task) => self.fetch_task = Some(task),
                    Err(err) => error("Failed to create", err),
                }
            }
            Msg::Success => {
                self.props.on_close.emit(());
                BackdropDispatcher::default().close()
            }
            Msg::NewDeviceName(name) => self.new_device_name = name,
        };
        true
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        true
    }

    fn view(&self) -> Html {
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
                variant= ModalVariant::Small
                footer = {html!{<>
                                <button class="pf-c-button pf-m-primary"
                                disabled=!is_valid || self.fetch_task.is_some()
                                type="button"
                                onclick=self.link.callback(|_|Msg::Create) >
                                    {"Create"}</button>
                         </>}}
            >
                <Form>
                       <FormGroup>
                            <TextInput
                                validator=Validator::from(v)
                                onchange=self.link.callback(|id|Msg::NewDeviceName(id))
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
    fn create(&self, name: String, app: String) -> Result<FetchTask, anyhow::Error> {
        let payload = json!({
        "metadata": {
            "name": name,
            "application": app
        },
        "spec": {},
        });

        self.props.backend.info.request(
            Method::POST,
            format!("/api/registry/v1alpha1/apps/{}/devices", url_encode(app)),
            Json(&payload),
            vec![("Content-Type", "application/json")],
            self.link
                .callback(move |response: Response<Text>| match response.status() {
                    StatusCode::CREATED => Msg::Success,
                    _ => Msg::Error(response.notify("Failed to create")),
                }),
        )
    }
}
