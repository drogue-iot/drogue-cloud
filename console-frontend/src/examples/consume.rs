use crate::{
    data::{SharedDataDispatcher, SharedDataOps},
    examples::{data::ExampleData, note_local_certs},
    html_prop,
};
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use yew::prelude::*;
use yew_oauth2::prelude::*;
use yew_router::{
    agent::{RouteAgentDispatcher, RouteRequest},
    route::Route,
};

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub endpoints: Endpoints,
    pub data: ExampleData,
    pub auth: Authentication,
}

pub struct ConsumeData {
    data_agent: SharedDataDispatcher<ExampleData>,
}

pub enum Msg {
    SetBinaryMode(bool),
    SetSharedConsumerMode(bool),
    SetConsumerGroup(String),
    SetDrgToken(bool),

    OpenSpy,
}

impl Component for ConsumeData {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {
            data_agent: SharedDataDispatcher::new(),
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::SetBinaryMode(binary_mode) => self
                .data_agent
                .update(move |data| data.binary_mode = binary_mode),
            Self::Message::SetSharedConsumerMode(shared_consumer_mode) => {
                self.data_agent
                    .update(move |data| match shared_consumer_mode {
                        true => data.consumer_group = Some(String::from("group-id")),
                        false => data.consumer_group = None,
                    })
            }
            Self::Message::SetConsumerGroup(group) => {
                self.data_agent
                    .update(|data| data.consumer_group = Some(group));
            }
            Self::Message::SetDrgToken(drg_token) => self
                .data_agent
                .update(move |data| data.drg_token = drg_token),
            Self::Message::OpenSpy => RouteAgentDispatcher::<()>::new()
                .send(RouteRequest::ChangeRoute(Route::new_default_state("/spy"))),
        }
        false
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let v = |ctx: ValidationContext<String>| match ctx.value.as_str() {
            "" => InputState::Error,
            v => {
                if v.chars().all(|c| c != '#' && c != '+' && c != '/') {
                    InputState::Default
                } else {
                    InputState::Error
                }
            }
        };

        let local_certs = ctx
            .props()
            .data
            .local_certs(ctx.props().endpoints.local_certs);

        let mut cards: Vec<_> = Vec::new();

        if let Some(mqtt) = &ctx.props().endpoints.mqtt_integration {
            let opts = match ctx.props().data.binary_mode {
                true => " -up content-mode=binary",
                false => "",
            };
            let topic = match ctx.props().data.consumer_group {
                None => format!("app/{app}", app = ctx.props().data.app_id),
                Some(ref group) => {
                    format!(
                        "$share/{group}/app/{app}",
                        group = group,
                        app = ctx.props().data.app_id
                    )
                }
            };
            let token = match ctx.props().data.drg_token {
                true => "\"$(drg whoami -t)\"".into(),
                false => format!("\"{}\"", ctx.props().auth.access_token),
            };
            let consume_mqtt_cmd = format!(
                r#"mqtt sub -h {host} -p {port} -s {certs}-t '{topic}' {opts} -pw {token}"#,
                host = mqtt.host,
                port = mqtt.port,
                token = token,
                topic = topic,
                opts = opts,
                certs = local_certs
                    .then(|| "--cafile build/certs/endpoints/root-cert.pem ")
                    .unwrap_or("")
            );
            cards.push(html!{
                <Card title={html_prop!({"Consume device data using MQTT"})}>
                    <div>
                        {"The data, published by devices, can also be consumed using MQTT."}
                    </div>
                    <div>
                        <Switch
                            checked={ctx.props().data.binary_mode}
                            label="Binary content mode" label_off="Structured content mode"
                            on_change={ctx.link().callback(Msg::SetBinaryMode)}
                            />
                    </div>
                    <div>
                        <Switch
                            checked={ctx.props().data.drg_token}
                            label="Use 'drg' to get the access token" label_off="Show current token in example"
                            on_change={ctx.link().callback(Msg::SetDrgToken)}
                            />
                    </div>
                    <div>
                        <Split gutter=true>
                            <SplitItem>
                                <div style="border-width: --pf-c-form-control--BorderWidth;">
                                <Switch
                                    checked={ctx.props().data.consumer_group.is_some()}
                                    label="Shared consumer: " label_off="Default consumer"
                                    on_change={ctx.link().callback(Msg::SetSharedConsumerMode)}
                                    />
                                </div>
                            </SplitItem>
                            <SplitItem>
                        if let Some(consumer_group) = &ctx.props().data.consumer_group {
                            <TextInput
                                value={consumer_group.to_owned()}
                                required=true
                                onchange={ctx.link().callback(Msg::SetConsumerGroup)}
                                validator={Validator::from(v)}
                                />
                        }
                            </SplitItem>
                        </Split>
                    </div>
                    <div>
                        {"Run the following command in a new terminal window:"}
                    </div>
                    <Clipboard code=true readonly=true variant={ClipboardVariant::Expandable} value={consume_mqtt_cmd} />
                    {note_local_certs(local_certs)}
                </Card>
            });
        }

        if let Some(ws) = &ctx.props().endpoints.websocket_integration {
            let token = match ctx.props().data.drg_token {
                true => "$(drg whoami -t)".into(),
                false => ctx.props().auth.access_token.clone(),
            };
            let consume_websocket_cmd = format!(
                r#"websocat {}/{} -H="Authorization: Bearer {}""#,
                ws.url,
                ctx.props().data.app_id,
                token,
            );
            let drg_cmd = format!("drg stream {}", ctx.props().data.app_id,);
            cards.push(html!{
                <Card title={html_prop!({"Consume device data using a Websocket"})}>
                    <div>
                        {"The data, published by devices, can also be consumed using a websocket."}
                    </div>
                    <div>
                        {"'drg' allows to easily get the stream:"}
                    </div>
                    <Clipboard code=true readonly=true variant={ClipboardVariant::Expandable} value={drg_cmd}/>
                    <div>
                        {"With a websocket client like 'websocat':"}
                    </div>
                    <div>
                        <Switch
                            checked={ctx.props().data.drg_token}
                            label="Use 'drg' to get the access token" label_off="Show current token in example"
                            on_change={ctx.link().callback(Msg::SetDrgToken)}
                            />
                    </div>
                    <div>
                        {"Run the following command in a new terminal window:"}
                    </div>
                    <Clipboard code=true readonly=true variant={ClipboardVariant::Expandable} value={consume_websocket_cmd} />
                </Card>
            });
        }

        let actions: Vec<Action> = vec![Action::new(
            "Try it!",
            ctx.link().callback(|_| Msg::OpenSpy),
        )];

        cards.push(html!{
            <Alert
                title="Spy Tool"
                r#type={Type::Info} inline=true
                actions={actions}
                >
                <Content>
                <p>
                    {"For quickly checking messages from devices, you can also use the \"Spy Tool\"."}
                </p>

                </Content>
            </Alert>
        });

        cards
            .iter()
            .map(|card| {
                html! {<StackItem> { card.clone() } </StackItem>}
            })
            .collect()
    }
}
