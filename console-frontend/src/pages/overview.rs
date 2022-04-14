use crate::html_prop;
use drogue_cloud_console_common::EndpointInformation;
use drogue_cloud_service_api::endpoints::MqttEndpoint;
use patternfly_yew::*;
use yew::{prelude::*, virtual_dom::VChild};

#[derive(Clone, Properties, PartialEq, Eq)]
pub struct Props {
    pub endpoints: EndpointInformation,
}

pub struct Overview {}

impl Component for Overview {
    type Message = ();
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! (
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Content>
                        <h1>{"Overview"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    { self.render_overview(ctx) }
                </PageSection>
            </>
        )
    }
}

impl Overview {
    fn render_overview(&self, ctx: &Context<Self>) -> Html {
        self.render_endpoints(&ctx.props().endpoints)
    }

    fn render_endpoints(&self, endpoints: &EndpointInformation) -> Html {
        let mut service_cards = Vec::new();
        let mut endpoint_cards = Vec::new();
        let mut integration_cards = Vec::new();
        let mut demo_cards = Vec::new();

        // Services column

        if let Some(api) = &endpoints.api {
            service_cards.push(self.render_linked_card(
                "API",
                api,
                Some(("/api", "Interactive API")),
            ));
            service_cards.push(self.render_linked_card(
                "Command line client",
                format!("drg login {}", api),
                Some((
                    "https://github.com/drogue-iot/drg/releases/latest",
                    "Download drg",
                )),
            ));
        }

        if let Some(sso) = &endpoints.sso {
            service_cards.push(self.render_card("Single sign-on", sso, true));
        }
        if let Some(kafka) = &endpoints.kafka_bootstrap_servers {
            service_cards.push(self.render_card("Kafka bootstrap servers", &kafka, false));
        }

        //endpoint column

        if let Some(coap) = &endpoints.coap {
            endpoint_cards.push(self.render_card("CoAP endpoint", &coap.url, false));
        }
        if let Some(http) = &endpoints.http {
            endpoint_cards.push(self.render_card("HTTP endpoint", &http.url, false));
        }
        if let Some(mqtt) = &endpoints.mqtt {
            endpoint_cards.push(self.render_mqtt_endpoint(mqtt, "MQTT endpoint"));
        }
        if let Some(mqtt_ws) = &endpoints.mqtt_ws {
            endpoint_cards.push(self.render_card(
                "MQTT over Websocket endpoint",
                &mqtt_ws.url,
                false,
            ));
        }
        if let Some(mqtt_ws_browser) = &endpoints.mqtt_ws_browser {
            endpoint_cards.push(self.render_card(
                "MQTT over Websocket endpoint for browser",
                &mqtt_ws_browser.url,
                false,
            ));
        }
        if let Some(url) = &endpoints.command_url {
            endpoint_cards.push(self.render_card("Command endpoint", url, false));
        }

        // integrations column

        if let Some(mqtt) = &endpoints.mqtt_integration {
            integration_cards.push(self.render_mqtt_endpoint(mqtt, "MQTT integration"));
        }
        if let Some(ws) = &endpoints.websocket_integration {
            integration_cards.push(self.render_card("Websocket integration", &ws.url, false));
        }
        if let Some(mqtt_ws) = &endpoints.mqtt_integration_ws {
            integration_cards.push(self.render_card(
                "MQTT over Websocket integration",
                &mqtt_ws.url,
                false,
            ));
        }
        if let Some(mqtt_ws_browser) = &endpoints.mqtt_integration_ws_browser {
            integration_cards.push(self.render_card(
                "MQTT over Websocket integration for browser",
                &mqtt_ws_browser.url,
                false,
            ));
        }

        for (label, url) in &endpoints.demos {
            demo_cards.push(self.render_card(label, url, true));
        }

        return html! {
            <Grid gutter=true>
                <GridItem cols={[3]}>
                    { Self::render_cards("Services", service_cards) }
                </GridItem>
                <GridItem cols={[3]}>
                    { Self::render_cards("Endpoints", endpoint_cards) }
                </GridItem>
                <GridItem cols={[3]}>
                    { Self::render_cards("Integrations", integration_cards) }
                </GridItem>
                <GridItem cols={[3]}>
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
                <Flex modifiers={[FlexModifier::Column.all(), FlexModifier::FullWidth.all()]}>
                    <FlexItem>
                        <Title size={Size::XLarge}>{label}</Title>
                    </FlexItem>
                    { cards.into_flex_items() }
                </Flex>
            </Flex>
        }
    }

    fn render_linked_card<S: Into<String>>(
        &self,
        label: &str,
        url: S,
        link: Option<(&str, &str)>,
    ) -> Html {
        let footer = {
            if let Some(link) = link {
                html! { <a class="pf-c-button pf-m-link pf-m-inline" href={link.0.to_string()} target="_blank"> { link.1 }</a> }
            } else {
                html! {}
            }
        };

        let title = html! {{label}};

        html! {
            <Card
                title={title}
                footer={footer}
                >
                <Clipboard readonly=true value={url.into()}/>
            </Card>
        }
    }

    fn render_card<S: AsRef<str>>(&self, label: &str, url: S, link: bool) -> Html {
        let url = url.as_ref();
        let link = match link {
            false => None,
            true => Some((url, label)),
        };
        self.render_linked_card(label, url, link)
    }

    fn render_mqtt_endpoint(&self, mqtt: &MqttEndpoint, label: &str) -> Html {
        html! {
            <Card
                title={html_prop!({label})}
                >
                <Clipboard readonly=true value={mqtt.host.clone()}/>
                <Clipboard readonly=true value={format!("{}", mqtt.port)}/>
            </Card>
        }
    }
}
