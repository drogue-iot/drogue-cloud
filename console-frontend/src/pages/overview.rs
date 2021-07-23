use crate::backend::Backend;
use anyhow::Error;
use drogue_cloud_service_api::endpoints::{Endpoints, MqttEndpoint};
use patternfly_yew::*;
use std::rc::Rc;
use yew::{
    format::{Json, Nothing},
    prelude::*,
    services::fetch::*,
    virtual_dom::VChild,
};

pub struct Overview {
    link: ComponentLink<Self>,

    ft: Option<FetchTask>,
    endpoints: Option<Endpoints>,
}

pub enum Msg {
    FetchOverview,
    FetchOverviewFailed,
    OverviewUpdate(Rc<Endpoints>),
}

impl Component for Overview {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchOverview);
        Self {
            ft: None,
            link,
            endpoints: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::FetchOverview => {
                self.ft = Some(self.fetch_overview().unwrap());
                true
            }
            Msg::OverviewUpdate(e) => {
                self.endpoints = Some((*e).clone());
                true
            }
            _ => false,
        }
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
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
    fn fetch_overview(&self) -> Result<FetchTask, Error> {
        Backend::request(
            Method::GET,
            "/api/console/v1alpha1/info",
            Nothing,
            self.link
                .callback(|response: Response<Json<Result<Endpoints, Error>>>| {
                    let parts = response.into_parts();
                    if let (meta, Json(Ok(body))) = parts {
                        if meta.status.is_success() {
                            return Msg::OverviewUpdate(Rc::new(body));
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

        for (label, url) in &endpoints.demos {
            demo_cards.push(self.render_card(label, url, true));
        }

        return html! {
            <Flex
                >
                { Self::render_cards("Services", service_cards) }
                { Self::render_cards("Endpoints", endpoint_cards) }
                { Self::render_cards("Integrations", integration_cards) }
                { if !demo_cards.is_empty() {
                    Self::render_cards("Demos", demo_cards)
                } else {
                    html_nested!{<Flex></Flex>}
                } }
            </Flex>
        };
    }

    fn render_cards(label: &str, cards: Vec<Html>) -> VChild<Flex> {
        html_nested! {
            <Flex>
                <Flex modifiers=vec![FlexModifier::Column.all()]>
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
