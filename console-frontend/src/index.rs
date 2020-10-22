use crate::Backend;
use anyhow::Error;
use console_common::{Endpoints, HttpEndpoint, MqttEndpoint};
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

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchOverview);
        Self {
            ft: None,
            link,
            endpoints: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
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

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Drogue IoT"}</h1>
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
        let request = Request::get(format!("{}/info", Backend::get().unwrap().url))
            .body(Nothing)
            .expect("Failed to build request");

        FetchService::fetch(
            request,
            self.link
                .callback(|response: Response<Json<Result<Endpoints, Error>>>| {
                    if let (meta, Json(Ok(body))) = response.into_parts() {
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
        html! {
            <Gallery
                gutter=true
                >
                {
                    match &Backend::get() {
                        Some(backend) => self.render_api_endpoint(backend),
                        None => html! {},
                    }
                }
                {
                    match &endpoints.http {
                        Some(http) => self.render_http_endpoint(http),
                        None => html! {},
                    }
                }
                {
                    match &endpoints.mqtt {
                        Some(mqtt) => self.render_mqtt_endpoint(mqtt),
                        None => html! {},
                    }
                }
            </Gallery>
        }
    }

    fn render_http_endpoint(&self, http: &HttpEndpoint) -> Html {
        html! {
            <Card
                title={html_nested!{<>{"HTTP Endpoint"}</>}}
                >
                <div>
                    { &http.url }
                </div>
            </Card>
        }
    }

    fn render_mqtt_endpoint(&self, mqtt: &MqttEndpoint) -> Html {
        html! {
            <Card
                title={html_nested!{<>{"MQTT Endpoint"}</>}}
                >
                <div>
                    { &mqtt.host } { ":" } { &mqtt.port }
                </div>
            </Card>
        }
    }

    fn render_api_endpoint(&self, backend: &Backend) -> Html {
        html! {
            <Card
                title={html_nested!{<>{"API Endpoint"}</>}}
                >
                <div>
                    { &backend.url }
                </div>
            </Card>
        }
    }
}
