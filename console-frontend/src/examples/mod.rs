use crate::backend::Backend;
use anyhow::Error;
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use serde_json::json;
use yew::prelude::*;
use yew::virtual_dom::VChild;
use yew::{
    format::{Json, Nothing},
    services::fetch::*,
};

#[derive(Debug, Clone, Default)]
pub struct Refs {
    app: NodeRef,
    device: NodeRef,
    password: NodeRef,
    payload: NodeRef,
}

pub struct Examples {
    link: ComponentLink<Self>,

    ft: Option<FetchTask>,
    endpoints: Option<Endpoints>,

    app_id: String,
    device_id: String,
    password: String,
    payload: String,

    binary_mode: bool,
    consumer_group: Option<String>,

    refs: Refs,
}

#[derive(Clone, Debug)]
pub enum Msg {
    FetchOverview,
    FetchOverviewFailed,
    OverviewUpdate(Endpoints),

    SetApplicationId(String),
    SetDeviceId(String),
    SetPassword(String),
    SetPayload(String),
    SetBinaryMode(bool),
    SetSharedConsumerMode(bool),
    SetConsumerGroup(String),
}

impl Component for Examples {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchOverview);
        Self {
            ft: None,
            link,
            endpoints: None,
            app_id: "app1".into(),
            device_id: "device1".into(),
            password: "hey-rodney".into(),
            payload: json!({"temp": 42}).to_string(),
            refs: Default::default(),
            binary_mode: false,
            consumer_group: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::FetchOverview => {
                self.ft = Some(self.fetch_overview().unwrap());
            }
            Msg::FetchOverviewFailed => return false,
            Msg::OverviewUpdate(e) => {
                self.endpoints = Some(e);
            }
            Msg::SetApplicationId(app) => self.app_id = app,
            Msg::SetDeviceId(device) => self.device_id = device,
            Msg::SetPassword(pwd) => self.password = pwd,
            Msg::SetPayload(payload) => self.payload = payload,
            Msg::SetBinaryMode(binary_mode) => self.binary_mode = binary_mode,
            Msg::SetSharedConsumerMode(shared_consumer_mode) => match shared_consumer_mode {
                true => self.consumer_group = Some(String::from("group-id")),
                false => self.consumer_group = None,
            },
            Msg::SetConsumerGroup(group) => {
                self.consumer_group = Some(group);
            }
        }
        true
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Examples"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    { self.render_overview() }
                </PageSection>
            </>
        };
    }
}

impl Examples {
    fn fetch_overview(&self) -> Result<FetchTask, Error> {
        Backend::request(
            Method::GET,
            "/api/v1/info",
            Nothing,
            self.link
                .callback(|response: Response<Json<Result<Endpoints, Error>>>| {
                    let parts = response.into_parts();
                    if let (meta, Json(Ok(body))) = parts {
                        if meta.status.is_success() {
                            return Msg::OverviewUpdate(body);
                        }
                    }
                    Msg::FetchOverviewFailed
                }),
        )
    }

    fn render_overview(&self) -> Html {
        match &self.endpoints {
            Some(endpoints) => self.render_endpoints(endpoints),
            None => html! {
                <div>{"Loading..."}</div>
            },
        }
    }

    fn render_endpoints(&self, endpoints: &Endpoints) -> Html {
        let v = |value: &str| match value {
            "" => InputState::Error,
            _ => InputState::Default,
        };

        return html! {
            <Flex>

                <FlexItem
                    modifiers=vec![FlexModifier::Flex1.into()]
                    >
                    <Stack gutter=true>

                        <StackItem>
                            <Title size=Size::XXLarge>{"Examples"}</Title>
                        </StackItem>

                        <StackItem>
                            <Alert
                                title="Requirements"
                                r#type=Type::Info inline=true
                                >
                                <Content>
                                <p>
                                    {"The following examples assume that you have "} <a href="https://httpie.io" target="_blank">{"HTTPie"}</a> {" and the "}
                                    <a href="https://hivemq.github.io/hivemq-mqtt-client/" target="_blank">{"MQTT client"}</a>
                                    {" installed. The commands are also expected to be executed in a Bash like shell."}
                                </p>

                                <p>{r#"Of course, it is possible to use another shell or HTTP/MQTT client with Drogue IoT. We simply wanted to keep the examples simple."#}</p>

                                </Content>
                            </Alert>
                        </StackItem>

                        { for self.render_examples(endpoints).iter().map(|c|{c.clone()}) }
                    </Stack>
                </FlexItem>

                <FlexItem modifiers=vec![FlexModifier::Flex1.into(), FlexModifier::Column.into()]>
                    <Stack gutter=true>
                        <StackItem>
                            <Title size=Size::XXLarge>{"Example Data"}</Title>
                        </StackItem>
                        <StackItem>
                            <Card title=html!{"App & Device"}>
                                <Form>
                                    <FormGroup label="Application ID">
                                        <TextInput
                                            ref=self.refs.app.clone()
                                            value=&self.app_id
                                            required=true
                                            onchange=self.link.callback(|app|Msg::SetApplicationId(app))
                                            validator=Validator::from(v)
                                            />
                                    </FormGroup>
                                    <FormGroup label="Device ID">
                                        <TextInput
                                            ref=self.refs.device.clone()
                                            value=&self.device_id
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
                                            ref=self.refs.password.clone()
                                            value=&self.password
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
                                        ref=self.refs.payload.clone()
                                        value=&self.payload
                                        onchange=self.link.callback(|payload|Msg::SetPayload(payload))
                                        validator=Validator::from(v)
                                        />
                                </Form>
                            </Card>
                        </StackItem>
                    </Stack>
                </FlexItem>

            </Flex>
        };
    }

    fn render_examples(&self, endpoints: &Endpoints) -> Vec<VChild<StackItem>> {
        let v = |value: &str| match value {
            "" => InputState::Error,
            v => {
                if v.chars().all(|c| c != '#' && c != '+' && c != '/') {
                    InputState::Default
                } else {
                    InputState::Error
                }
            }
        };

        let mut cards: Vec<_> = Vec::new();

        if let Some(registry) = &endpoints.registry {
            let create_app_cmd = format!(
                r#"http POST {url}/api/v1/apps metadata:='{meta}'"#,
                url = registry.url,
                meta = json!({ "name": self.app_id })
            );
            let create_device_cmd = format!(
                r#"http POST {url}/api/v1/apps/{app}/devices metadata:='{meta}' spec:='{spec}'"#,
                app = self.app_id,
                url = registry.url,
                meta = shell_quote(json!({"application": self.app_id, "name": self.device_id})),
                spec = shell_quote(json!({"credentials": {"credentials":[
                    {"pass": self.password},
                ]}})),
            );
            cards.push(html_nested!{
                <Card title={html!{"Create a new application"}}>
                    <div>
                    {"As a first step, you will need to create a new application."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=create_app_cmd/>
                </Card>
            });
            cards.push(html_nested!{
                <Card title={html!{"Create a new device"}}>
                    <div>
                    {"As part of your application, you can then create a new device."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=create_device_cmd/>
                </Card>
            });
        }

        if let Some(mqtt) = &endpoints.mqtt_integration {
            let opts = match self.binary_mode {
                true => " -up content-mode=binary",
                false => "",
            };
            let topic = match self.consumer_group {
                None => format!("app/{app}", app = self.app_id),
                Some(ref group) => {
                    format!(
                        "$shared/{group}/app/{app}",
                        group = group,
                        app = self.app_id
                    )
                }
            };
            let consume_mqtt_cmd = format!(
                r#"mqtt sub -h {host} -p {port} -s -t '{topic}' {opts} -pw "{token}""#,
                host = mqtt.host,
                port = mqtt.port,
                token = self.token(),
                topic = topic,
                opts = opts,
            );
            cards.push(html_nested!{
                <Card title=html!{"Consume device data using MQTT"}>
                    <div>
                        {"The data, published by devices, can also be consumed using MQTT."}
                    </div>
                    <div>
                        <Switch
                            checked=self.binary_mode
                            label="Binary content mode" label_off="Structured content mode"
                            on_change=self.link.callback(|data| Msg::SetBinaryMode(data))
                            />
                    </div>
                    <div>
                        <Split gutter=true>
                            <SplitItem>
                            <div style="border-width: --pf-c-form-control--BorderWidth;">
                        <Switch
                            checked=self.consumer_group.is_some()
                            label="Shared consumer: " label_off="Default consumer"
                            on_change=self.link.callback(|data| Msg::SetSharedConsumerMode(data))
                            />
                            </div></SplitItem>
                            <SplitItem>
                        { if let Some(ref consumer_group) = self.consumer_group {html!{
                            <TextInput
                                value=consumer_group
                                required=true
                                onchange=self.link.callback(|consumer_group|Msg::SetConsumerGroup(consumer_group))
                                validator=Validator::from(v)
                                />
                        }} else { html!{}} }
                            </SplitItem>
                        </Split>
                    </div>
                    <div>
                        {"Run the following command in a new terminal window:"}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=consume_mqtt_cmd/>
                </Card>
            });
        }

        if let Some(http) = &endpoints.http {
            let publish_http_cmd = format!(
                "echo '{payload}' | http --auth '{auth}' POST {url}/v1/foo",
                payload = shell_quote(&self.payload),
                url = http.url,
                auth = shell_quote(format!(
                    "{device_id}@{app_id}:{password}",
                    app_id = self.app_id,
                    device_id = url_encode(&self.device_id),
                    password = &self.password,
                )),
            );
            cards.push(html_nested!{
                <Card title={html!{"Publish data using HTTP"}}>
                    <div>
                        {"You can now publish data to the cloud using HTTP."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=publish_http_cmd/>
                </Card>
            });
        }

        if let Some(mqtt) = &endpoints.mqtt {
            let publish_mqtt_cmd = format!(
                r#"mqtt pub -v -h {host} -p {port} -u '{device_id}@{app_id}' -pw '{password}' -s -t temp -m '{payload}'"#,
                host = mqtt.host,
                port = mqtt.port,
                app_id = &self.app_id,
                device_id = shell_quote(url_encode(&self.device_id)),
                password = shell_quote(&self.password),
                payload = shell_quote(&self.payload)
            );
            cards.push(html_nested!{
                <Card title={html!{"Publish data using MQTT"}}>
                    <div>
                        {"You can now publish data to the cloud using MQTT."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=publish_mqtt_cmd/>
                </Card>
            });
        }

        cards
            .iter()
            .map(|card| {
                html_nested! {<StackItem> { card.clone() } </StackItem>}
            })
            .collect()
    }

    fn token(&self) -> String {
        Backend::token()
            .map(|token| token.access_token)
            .unwrap_or_default()
    }
}

fn shell_quote<S: ToString>(s: S) -> String {
    s.to_string().replace('\\', "\\\\").replace('\'', "\\\'")
}

fn url_encode<S: AsRef<str>>(s: S) -> String {
    percent_encoding::utf8_percent_encode(s.as_ref(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
}
