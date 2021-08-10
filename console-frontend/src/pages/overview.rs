use crate::backend::Backend;
use drogue_cloud_service_api::endpoints::{Endpoints, MqttEndpoint};
use patternfly_yew::*;
use yew::{prelude::*, virtual_dom::VChild};

#[derive(Clone, Properties, PartialEq, Eq)]
pub struct Props {
    pub endpoints: Option<Endpoints>,
}

pub struct Overview {
    props: Props,
}

impl Component for Overview {
    type Message = ();
    type Properties = Props;

    fn create(props: Self::Properties, _: ComponentLink<Self>) -> Self {
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
        html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Overview"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    { self.render_overview() }
                </PageSection>
            </>
        }
    }
}

impl Overview {
    fn render_overview(&self) -> Html {
        match &self.props.endpoints {
            Some(endpoints) => self.render_endpoints(endpoints),
            None => html! {
                <div>{"Loading..."}</div>
            },
        }
    }

    fn render_endpoints(&self, endpoints: &Endpoints) -> Html {
        let mut service_cards = Vec::new();
        let mut endpoint_cards = Vec::new();
        let mut integration_cards = Vec::new();
        let mut demo_cards = Vec::new();

        if let Some(backend) = Backend::get() {
            service_cards.push(self.render_card("API", backend.current_url(), false));
        }
        if let Some(sso) = &endpoints.sso {
            service_cards.push(self.render_card("Single sign-on", sso, true));
        }
        if let Some(registry) = &endpoints.registry {
            service_cards.push(self.render_card("Device registry", &registry.url, false));
        }
        if let Some(coap) = &endpoints.coap {
            endpoint_cards.push(self.render_card("CoAP endpoint", &coap.url, false));
        }
        if let Some(http) = &endpoints.http {
            endpoint_cards.push(self.render_card("HTTP endpoint", &http.url, false));
        }
        if let Some(mqtt) = &endpoints.mqtt {
            endpoint_cards.push(self.render_mqtt_endpoint(&mqtt, "MQTT endpoint"));
        }
        if let Some(url) = &endpoints.command_url {
            endpoint_cards.push(self.render_card("Command endpoint", url, false));
        }

        if let Some(mqtt) = &endpoints.mqtt_integration {
            integration_cards.push(self.render_mqtt_endpoint(&mqtt, "MQTT integration"));
        }

        if let Some(ws) = &endpoints.websocket_integration {
            integration_cards.push(self.render_card("Websocket integration", &ws.url, false));
        }

        for (label, url) in &endpoints.demos {
            demo_cards.push(self.render_card(label, url, true));
        }

        return html! {
            <Grid gutter=true>
                <GridItem cols=[3]>
                    { Self::render_cards("Services", service_cards) }
                </GridItem>
                <GridItem cols=[3]>
                    { Self::render_cards("Endpoints", endpoint_cards) }
                </GridItem>
                <GridItem cols=[3]>
                    { Self::render_cards("Integrations", integration_cards) }
                </GridItem>
                <GridItem cols=[3]>
                    { if !demo_cards.is_empty() {
                        Self::render_cards("Demos", demo_cards)
                    } else {
                        html_nested!{<Flex></Flex>}
                    } }
                </GridItem>
            </Grid>
        };
    }

    fn render_cards(label: &str, cards: Vec<Html>) -> VChild<Flex> {
        html_nested! {
            <Flex>
                <Flex modifiers=vec![FlexModifier::Column.all(), FlexModifier::FullWidth.all()]>
                    <FlexItem>
                        <Title size=Size::XLarge>{label}</Title>
                    </FlexItem>
                    { cards.into_flex_items() }
                </Flex>
            </Flex>
        }
    }

    fn render_card<S: AsRef<str>>(&self, label: &str, url: S, link: bool) -> Html {
        let footer = {
            if link {
                html! { <a class="pf-c-button pf-m-link pf-m-inline" href=url.as_ref() target="_blank"> { label }</a> }
            } else {
                html! {}
            }
        };

        html! {
            <Card
                title={html_nested!{<>{label}</>}}
                footer=footer
                >
                <Clipboard readonly=true value=url.as_ref()/>
            </Card>
        }
    }

    fn render_mqtt_endpoint(&self, mqtt: &MqttEndpoint, label: &str) -> Html {
        html! {
            <Card
                title={html_nested!{<>{label}</>}}
                >
                <Clipboard readonly=true value=&mqtt.host/>
                <Clipboard readonly=true value={format!("{}", mqtt.port)}/>
            </Card>
        }
    }
}
