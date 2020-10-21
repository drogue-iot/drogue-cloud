#![recursion_limit = "512"]

use anyhow::Error;

use patternfly_yew::*;

use headers::authorization::Credentials;
use headers::Authorization;

use wasm_bindgen::prelude::*;

use chrono_tz::Europe::Berlin;

use chrono::Utc;
use yew::format::{Json, Nothing};
use yew::prelude::*;
use yew::services::fetch::{FetchService, FetchTask, Request, Response};

struct Model {
    link: ComponentLink<Self>,
}

pub enum Msg {}

impl Component for Model {
    type Message = Msg;
    type Properties = ();
    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self { link }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        //match msg {}
        true
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        html! {
            <Page>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Dorgue IoT"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    <div>{"Needs more work"}</div>
                </PageSection>
            </Page>
        }
    }
}

impl Model {}

#[wasm_bindgen(start)]
pub fn run_app() {
    App::<Model>::new().mount_to_body();
}
