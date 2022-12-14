use crate::{backend::BackendInformation, components::spy::Spy};
use drogue_cloud_console_common::EndpointInformation;
use yew::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Properties)]
pub struct Props {
    pub backend: BackendInformation,
    pub endpoints: EndpointInformation,
    pub application: String,
    pub device: String,
}

#[function_component(Debug)]
pub fn debug(props: &Props) -> Html {
    html! (
        <Spy
            backend={props.backend.clone()}
            endpoints={props.endpoints.clone()}
            application={props.application.clone()}
            device={props.device.clone()}
        />
    )
}
