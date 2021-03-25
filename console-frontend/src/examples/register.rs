use super::shell_quote;
use crate::{backend::Token, examples::data::ExampleData};
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use serde_json::json;
use yew::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq, Eq)]
pub struct Props {
    pub endpoints: Endpoints,
    pub data: ExampleData,
    pub token: Token,
}

pub struct RegisterDevices {
    props: Props,
}

impl Component for RegisterDevices {
    type Message = ();
    type Properties = Props;

    fn create(props: Self::Properties, _link: ComponentLink<Self>) -> Self {
        Self { props }
    }

    fn update(&mut self, _msg: Self::Message) -> ShouldRender {
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
        let mut cards: Vec<_> = Vec::new();

        cards.push(html!{
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
        });

        if let Some(registry) = &self.props.endpoints.registry {
            let create_app_cmd = format!(
                r#"http POST {url}/api/v1/apps metadata:='{meta}'"#,
                url = registry.url,
                meta = json!({ "name": self.props.data.app_id })
            );
            let create_device_cmd = format!(
                r#"http POST {url}/api/v1/apps/{app}/devices metadata:='{meta}' spec:='{spec}'"#,
                app = self.props.data.app_id,
                url = registry.url,
                meta = shell_quote(
                    json!({"application": self.props.data.app_id, "name": self.props.data.device_id})
                ),
                spec = shell_quote(json!({"credentials": {"credentials":[
                    {"pass": self.props.data.password},
                ]}})),
            );
            cards.push(html!{
                <Card title={html!{"Create a new application"}}>
                    <div>
                    {"As a first step, you will need to create a new application."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=create_app_cmd/>
                </Card>
            });
            cards.push(html!{
                <Card title={html!{"Create a new device"}}>
                    <div>
                    {"As part of your application, you can then create a new device."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=create_device_cmd/>
                </Card>
            });
        }

        cards
            .iter()
            .map(|card| {
                html! {<StackItem> { card.clone() } </StackItem>}
            })
            .collect()
    }
}
