use crate::backend::Backend;
use anyhow::Error;
use drogue_cloud_service_api::endpoints::{Endpoints, HttpEndpoint, MqttEndpoint};
use patternfly_yew::*;
use yew::format::{Json, Nothing};
use yew::prelude::*;
use yew::services::fetch::*;

pub struct Index {
    link: ComponentLink<Self>,

    ft: Option<FetchTask>,
    endpoints: Option<Endpoints>,
}

pub enum Msg {
    FetchOverview,
    FetchOverviewFailed,
    OverviewUpdate(Endpoints),
}

impl Component for Index {
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
                self.endpoints = Some(e);
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

impl Index {
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
        let mut cards = Vec::new();

        if let Some(backend) = Backend::get() {
            cards.push(self.render_api_endpoint(&backend));
        }

        if let Some(http) = &endpoints.http {
            cards.push(self.render_http_endpoint(&http));
        }

        if let Some(mqtt) = &endpoints.mqtt {
            cards.push(self.render_mqtt_endpoint(&mqtt));
        }

        let cards: Vec<FlexChildVariant> = cards
            .iter()
            .map(|card| {
                return html_nested! {
                    <FlexItem>
                        {card.clone()}
                    </FlexItem>
                }
                .into();
            })
            .collect();

        return html! {
            <Flex>
                { cards }
            </Flex>
        };
    }

    fn render_http_endpoint(&self, http: &HttpEndpoint) -> Html {
        html! {
            <Card
                title={html_nested!{<>{"HTTP Endpoint"}</>}}
                >
                <Clipboard value=&http.url/>
            </Card>
        }
    }

    fn render_mqtt_endpoint(&self, mqtt: &MqttEndpoint) -> Html {
        html! {
            <Card
                title={html_nested!{<>{"MQTT Endpoint"}</>}}
                >
                <Clipboard value=&mqtt.host/>
                <Clipboard value={format!("{}", mqtt.port)}/>
            </Card>
        }
    }

    fn render_api_endpoint(&self, backend: &Backend) -> Html {
        html! {
            <Card
                title={html_nested!{<>{"API Endpoint"}</>}}
                >
                <Clipboard value=backend.current_url()/>
            </Card>
        }
    }
}
