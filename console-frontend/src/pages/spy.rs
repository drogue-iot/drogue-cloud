use crate::backend::BackendInformation;
use drogue_cloud_console_common::EndpointInformation;
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct Props {
    pub backend: BackendInformation,
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
                        endpoints={props.endpoints.clone()}
                    />

            </PageSection>
        </>
    }
}
