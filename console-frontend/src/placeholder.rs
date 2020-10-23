use patternfly_yew::*;
use yew::prelude::*;

pub struct Placeholder {}

impl Component for Placeholder {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, _: ComponentLink<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, _: Self::Message) -> bool {
        false
    }

    fn change(&mut self, _: Self::Properties) -> bool {
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
                    <div></div>
                </PageSection>
            </>
        }
    }
}
