use crate::{
    backend::Token,
    data::{SharedDataDispatcher, SharedDataOps},
    examples::{data::ExampleData, note_local_certs},
};
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use yew::prelude::*;
use yew_router::{
    agent::{RouteAgentDispatcher, RouteRequest},
    route::Route,
};

#[derive(Clone, Debug, Properties, PartialEq, Eq)]
pub struct Props {
    pub endpoints: Endpoints,
    pub data: ExampleData,
    pub token: Token,
}

pub struct ConsumeData {
    props: Props,
    link: ComponentLink<Self>,

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

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            props,
            link,
            data_agent: SharedDataDispatcher::new(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
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

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.props != props {
            self.props = props;
            true
        } else {
            false
        }
    }

    fn view(&self) -> Html {
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

        let local_certs = self
            .props
            .data
            .local_certs(self.props.endpoints.local_certs);

        let mut cards: Vec<_> = Vec::new();

        if let Some(mqtt) = &self.props.endpoints.mqtt_integration {
            let opts = match self.props.data.binary_mode {
                true => " -up content-mode=binary",
                false => "",
            };
            let topic = match self.props.data.consumer_group {
                None => format!("app/{app}", app = self.props.data.app_id),
                Some(ref group) => {
                    format!(
                        "$shared/{group}/app/{app}",
                        group = group,
                        app = self.props.data.app_id
                    )
                }
            };
            let token = match self.props.data.drg_token {
                true => "\"$(drg token)\"".into(),
                false => format!("\"{}\"", self.props.token.access_token),
            };
            let consume_mqtt_cmd = format!(
                r#"mqtt sub -h {host} -p {port} -s {certs}-t '{topic}' {opts} -pw {token}"#,
                host = mqtt.host,
                port = mqtt.port,
                token = token,
                topic = topic,
                opts = opts,
                certs = local_certs
                    .then(|| "--cafile build/certs/endpoints/ca-bundle.pem")
                    .unwrap_or("")
            );
            cards.push(html!{
                <Card title=html!{"Consume device data using MQTT"}>
                    <div>
                        {"The data, published by devices, can also be consumed using MQTT."}
                    </div>
                    <div>
                        <Switch
                            checked=self.props.data.binary_mode
                            label="Binary content mode" label_off="Structured content mode"
                            on_change=self.link.callback(|data| Msg::SetBinaryMode(data))
                            />
                    </div>
                    <div>
                        <Switch
                            checked=self.props.data.drg_token
                            label="Use 'drg token' to get the access token" label_off="Show current token in example"
                            on_change=self.link.callback(|data| Msg::SetDrgToken(data))
                            />
                    </div>
                    <div>
                        <Split gutter=true>
                            <SplitItem>
                            <div style="border-width: --pf-c-form-control--BorderWidth;">
                        <Switch
                            checked=self.props.data.consumer_group.is_some()
                            label="Shared consumer: " label_off="Default consumer"
                            on_change=self.link.callback(|data| Msg::SetSharedConsumerMode(data))
                            />
                            </div></SplitItem>
                            <SplitItem>
                        { if let Some(ref consumer_group) = self.props.data.consumer_group {html!{
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
                    {note_local_certs(local_certs)}
                </Card>
            });
        }

        let actions: Vec<Action> =
            vec![Action::new("Try it!", self.link.callback(|_| Msg::OpenSpy))];

        cards.push(html!{
            <Alert
                title="Spy Tool"
                r#type=Type::Info inline=true
                actions=actions
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
