use crate::data::{self, SharedDataBridge, SharedDataOps};
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

    pub cmd_empty_message: bool,
    pub cmd_name: String,
    pub cmd_payload: String,
}

impl Default for ExampleData {
    fn default() -> Self {
        Self {
            app_id: "app1".into(),
            device_id: "device1".into(),
            password: "hey-rodney".into(),
            payload: json!({"temp": 42}).to_string(),

            binary_mode: false,
            consumer_group: None,

            drg_token: true,

            cmd_empty_message: false,
            cmd_name: "set-temp".into(),
            cmd_payload: json!({"target-temp": 23}).to_string(),
        }
    }
}

#[derive(Clone, Debug, Properties, PartialEq, Eq)]
pub struct Props {}

#[derive(Clone, Debug)]
pub enum Msg {
    SetData(ExampleData),

    SetApplicationId(String),
    SetDeviceId(String),
    SetPassword(String),
    SetPayload(String),
}

pub struct CoreExampleData {
    props: Props,
    link: ComponentLink<Self>,

    data: Option<ExampleData>,
    data_agent: SharedDataBridge<ExampleData>,
}

impl Component for CoreExampleData {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let data_callback = link.batch_callback(|output| match output {
            data::Response::State(data) => vec![Msg::SetData(data)],
        });
        let mut data_agent = SharedDataBridge::new(data_callback);
        data_agent.request_state();

        Self {
            props,
            link,
            data: None,
            data_agent,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::SetApplicationId(app) => self.data_agent.update(|mut data| data.app_id = app),
            Msg::SetDeviceId(device) => self.data_agent.update(|mut data| data.device_id = device),
            Msg::SetPassword(pwd) => self.data_agent.update(|mut data| data.password = pwd),
            Msg::SetPayload(payload) => self.data_agent.update(|mut data| data.payload = payload),
            Msg::SetData(data) => self.data = Some(data),
        }
        true
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.props != props {
            self.props = props;
            true
        } else {
            false
        }
    }

    fn view(&self) -> Html {
        match &self.data {
            Some(data) => self.render_view(data),
            _ => html! {},
        }
    }
}

impl CoreExampleData {
    fn render_view(&self, data: &ExampleData) -> Html {
        let v = |value: &str| match value {
            "" => InputState::Error,
            _ => InputState::Default,
        };

        return html! {
            <Stack gutter=true>
                <StackItem>
                    <Title size=Size::XXLarge>{"Example Data"}</Title>
                </StackItem>
                <StackItem>
                    <Card title=html!{"App & Device"}>
                        <Form>
                            <FormGroup label="Application ID">
                                <TextInput
                                    value=&data.app_id
                                    required=true
                                    onchange=self.link.callback(|app|Msg::SetApplicationId(app))
                                    validator=Validator::from(v)
                                    />
                            </FormGroup>
                            <FormGroup label="Device ID">
                                <TextInput
                                    value=&data.device_id
                                    required=true
                                    onchange=self.link.callback(|device|Msg::SetDeviceId(device))
                                    validator=Validator::from(v)
                                    />
                            </FormGroup>
                        </Form>
                    </Card>
                </StackItem>
                <StackItem>
                    <Card title=html!{"Credentials"}>
                        <Form>
                            <FormGroup label="Password">
                                <TextInput
                                    value=&data.password
                                    required=true
                                    onchange=self.link.callback(|password|Msg::SetPassword(password))
                                    validator=Validator::from(v)
                                    />
                            </FormGroup>
                        </Form>
                    </Card>
                </StackItem>
                <StackItem>
                    <Card title=html!{"Payload"}>
                        <Form>
                            <TextArea
                                value=&data.payload
                                onchange=self.link.callback(|payload|Msg::SetPayload(payload))
                                validator=Validator::from(v)
                                />
                        </Form>
                    </Card>
                </StackItem>
            </Stack>
        };
    }
}
