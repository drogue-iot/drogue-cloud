use crate::backend::{Backend, Token};
use drogue_cloud_console_common::EndpointInformation;
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub backend: Backend,
    pub token: Token,
    pub endpoints: EndpointInformation,
}

pub struct Spy {
    props: Props,
}

impl Component for Spy {
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
        html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <Title>{"Device Message Spy"}</Title>
                    </Content>
                </PageSection>
                <PageSection>

                    <crate::components::spy::Spy
                            backend=&self.props.backend
                            token=&self.props.token
                            endpoints=&self.props.endpoints
                        />

                </PageSection>
            </>
        }
    }
}
