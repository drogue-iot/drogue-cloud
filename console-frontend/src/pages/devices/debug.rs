use crate::{backend::BackendInformation, components::spy::Spy};
use drogue_cloud_console_common::EndpointInformation;
use yew::prelude::*;

#[derive(Clone, Debug, PartialEq, Properties)]
pub struct Props {
    pub backend: BackendInformation,
    pub endpoints: EndpointInformation,
    pub application: String,
    pub device: String,
}

pub enum Msg {}

pub struct Debug {}

impl Component for Debug {
    type Message = Msg;
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! (
            <Spy
                backend={ctx.props().backend.clone()}
                endpoints={ctx.props().endpoints.clone()}
                application={ctx.props().application.clone()}
                device={ctx.props().device.clone()}
                />
        )
    }
}
