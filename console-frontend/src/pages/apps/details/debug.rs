use crate::{
    backend::{Backend, Token},
    components::spy::Spy,
};
use drogue_cloud_console_common::EndpointInformation;
use yew::prelude::*;

#[derive(Clone, Debug, PartialEq, Properties)]
pub struct Props {
    pub backend: Backend,
    pub endpoints: EndpointInformation,
    pub token: Token,
    pub application: String,
}

pub enum Msg {}

pub struct Debug {
    props: Props,
}

impl Component for Debug {
    type Message = Msg;
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
        return html! {
            <Spy
                backend=&self.props.backend
                token=&self.props.token
                endpoints=&self.props.endpoints
                application=&self.props.application
                />
        };
    }
}
