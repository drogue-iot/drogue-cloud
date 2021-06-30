use crate::{
    examples::{data::ExampleData, note_local_certs, shell_quote, shell_single_quote},
    utils::url_encode,
};
use drogue_cloud_service_api::endpoints::Endpoints;
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq, Eq)]
pub struct Props {
    pub endpoints: Endpoints,
    pub data: ExampleData,
}

pub struct PublishData {
    props: Props,
}

impl Component for PublishData {
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

        let local_certs = self
            .props
            .data
            .local_certs(self.props.endpoints.local_certs);

        if let Some(http) = &self.props.endpoints.http {
            let publish_http_cmd = format!(
                "echo '{payload}' | http --auth '{auth}' {certs}POST {url}/v1/foo",
                payload = shell_quote(&self.props.data.payload),
                url = http.url,
                auth = shell_quote(format!(
                    "{device_id}@{app_id}:{password}",
                    app_id = self.props.data.app_id,
                    device_id = url_encode(&self.props.data.device_id),
                    password = &self.props.data.password,
                )),
                certs = local_certs
                    .then(|| "--verify build/certs/endpoints/root-cert.pem ")
                    .unwrap_or("")
            );
            cards.push(html!{
                <Card title={html!{"Publish data using HTTP"}}>
                    <div>
                        {"You can now publish data to the cloud using HTTP."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=publish_http_cmd/>
                    {note_local_certs(local_certs)}
                </Card>
            });
        }

        if let Some(mqtt) = &self.props.endpoints.mqtt {
            let publish_mqtt_cmd = format!(
                r#"mqtt pub -h {host} -p {port} -u '{device_id}@{app_id}' -pw '{password}' -s {certs}-t temp -m {payload}"#,
                host = mqtt.host,
                port = mqtt.port,
                app_id = &self.props.data.app_id,
                device_id = shell_quote(url_encode(&self.props.data.device_id)),
                password = shell_quote(&self.props.data.password),
                payload = shell_single_quote(&self.props.data.payload),
                certs = local_certs
                    .then(|| "--cafile build/certs/endpoints/root-cert.pem ")
                    .unwrap_or("")
            );
            cards.push(html!{
                <Card title={html!{"Publish data using MQTT"}}>
                    <div>
                        {"You can now publish data to the cloud using MQTT."}
                    </div>
                    <Clipboard code=true readonly=true variant=ClipboardVariant::Expandable value=publish_mqtt_cmd/>
                    {note_local_certs(local_certs)}
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
