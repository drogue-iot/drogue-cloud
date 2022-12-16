use crate::utils::context::{ContextListener, MutableContext};
use crate::utils::not_empty;
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use serde_json::json;
use yew::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExampleData {
    pub app_id: String,
    pub device_id: String,
    pub password: String,
    pub payload: String,

    pub binary_mode: bool,
    pub consumer_group: Option<String>,

    pub drg_token: bool,
    // whether to use a local certificate
    pub enable_local_cert: bool,

    pub cmd_empty_message: bool,
    pub cmd_name: String,
    pub cmd_payload: String,
}

impl Default for ExampleData {
    fn default() -> Self {
        Self {
            app_id: "example-app".into(),
            device_id: "device1".into(),
            password: "hey-rodney".into(),
            payload: json!({"temp": 42}).to_string(),

            binary_mode: false,
            consumer_group: None,

            drg_token: true,
            enable_local_cert: true,

            cmd_empty_message: false,
            cmd_name: "set-temp".into(),
            cmd_payload: json!({"target-temp": 23}).to_string(),
        }
    }
}

impl ExampleData {
    pub fn local_certs(&self, offer_local_certs: bool) -> bool {
        offer_local_certs && self.enable_local_cert
    }
}

#[derive(Clone, Debug, Properties, PartialEq, Eq)]
pub struct Props {
    pub endpoints: Endpoints,
}

#[derive(Clone, Debug)]
pub enum Msg {
    ApplicationId(String),
    DeviceId(String),
    Password(String),
    Payload(String),
    LocalCerts(bool),
}

pub struct CoreExampleData {
    data: ContextListener<MutableContext<ExampleData>>,
}

impl Component for CoreExampleData {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            data: ContextListener::unwrap(ctx),
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ApplicationId(app) => self.data.get().update(|mut data| data.app_id = app),
            Msg::DeviceId(device) => self.data.get().update(|mut data| data.device_id = device),
            Msg::Password(pwd) => self.data.get().update(|mut data| data.password = pwd),
            Msg::Payload(payload) => self.data.get().update(|mut data| data.payload = payload),
            Msg::LocalCerts(local_certs) => self
                .data
                .get()
                .update(move |mut data| data.enable_local_cert = local_certs),
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let title_app = html! {"App & Device"};
        let title_creds = html! {"Credentials"};
        let title_payload = html! {"Payload"};
        let title_certs = html! {"Certificates"};

        let data = &self.data.get().context;

        html! {
            <Stack gutter=true>
                <StackItem>
                    <Title size={Size::XXLarge}>{"Example Data"}</Title>
                </StackItem>
                <StackItem>
                    <Card title={title_app}>
                        <Form>
                            <FormGroup label="Application ID">
                                <TextInput
                                    value={data.app_id.clone()}
                                    required=true
                                    onchange={ctx.link().callback(Msg::ApplicationId)}
                                    validator={not_empty()}
                                    />
                            </FormGroup>
                            <FormGroup label="Device ID">
                                <TextInput
                                    value={data.device_id.clone()}
                                    required=true
                                    onchange={ctx.link().callback(Msg::DeviceId)}
                                    validator={not_empty()}
                                    />
                            </FormGroup>
                        </Form>
                    </Card>
                </StackItem>
                <StackItem>
                    <Card title={title_creds}>
                        <Form>
                            <FormGroup label="Password">
                                <TextInput
                                    value={data.password.clone()}
                                    required=true
                                    onchange={ctx.link().callback(Msg::Password)}
                                    validator={not_empty()}
                                    />
                            </FormGroup>
                        </Form>
                    </Card>
                </StackItem>
                <StackItem>
                    <Card title={title_payload}>
                        <Form>
                            <TextArea
                                value={data.payload.clone()}
                                onchange={ctx.link().callback(Msg::Payload)}
                                validator={not_empty()}
                                />
                        </Form>
                    </Card>
                </StackItem>

            if ctx.props().endpoints.local_certs {
                <StackItem>
                    <Card title={title_certs}>
                        <Switch
                            checked={data.enable_local_cert}
                            label="Use local test certificates"
                            label_off="Use system default certificates"
                            on_change={ctx.link().callback(Msg::LocalCerts)}
                            />
                    </Card>
                </StackItem>
            }

            </Stack>
        }
    }
}
