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

#[function_component(Spy)]
pub fn spy(props: &Props) -> Html {
    html! {
        <>
            <PageSection variant={PageSectionVariant::Light} limit_width=true>
                <Content>
                    <Title>{"Device Message Spy"}</Title>
                </Content>
            </PageSection>
            <PageSection>

                <crate::components::spy::Spy
                        backend={props.backend.clone()}
                        token={props.token.clone()}
                        endpoints={props.endpoints.clone()}
                    />

            </PageSection>
        </>
    }
}
