use crate::{
    backend::Backend,
    examples::data::ExampleData,
    utils::{shell_quote, shell_single_quote},
};
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use serde_json::json;
use yew::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq, Eq)]
pub struct Props {
    pub endpoints: Endpoints,
    pub data: ExampleData,
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
        let mut cards: Vec<_> = vec![html! {
            <Alert
                title="Requirements"
                r#type=Type::Info inline=true
                >
                <Content>
                <p>
                    {"The following examples assume that you have the "}
                    <a href="https://github.com/drogue-iot/drg" target="_blank">{"Drogue Command Line Client"}</a>{", "}
                    <a href="https://httpie.io" target="_blank">{"HTTPie"}</a> {", and the "}
                    <a href="https://hivemq.github.io/hivemq-mqtt-client/" target="_blank">{"MQTT client"}</a>
                    {" installed. The commands are also expected to be executed in a Bash like shell."}
                </p>

                <p>{r#"Of course, it is possible to use another shell or HTTP/MQTT client with Drogue IoT. We simply wanted to keep the examples simple."#}</p>

                </Content>
            </Alert>
        }];

        if let Some(api) = Backend::url("") {
            let login_cmd = format!(r#"drg login {url}"#, url = shell_quote(api));
            cards.push(html!{
                <Card title=html!{"Log in"}>
                    <div>
                    {"Log in to the backend. This will ask you to open the login URL in the browser, in order to follow the OpenID Connect login flow."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=login_cmd/>
                </Card>
            });
        }

        let create_app_cmd = format!(r#"drg create app {name}"#, name = self.props.data.app_id);
        let create_device_cmd = format!(
            r#"drg create device --app {app} {device} --data {spec}"#,
            app = self.props.data.app_id,
            device = shell_quote(&self.props.data.device_id),
            spec = shell_single_quote(json!({"credentials": {"credentials":[
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

        cards
            .iter()
            .map(|card| {
                html! {<StackItem> { card.clone() } </StackItem>}
            })
            .collect()
    }
}
