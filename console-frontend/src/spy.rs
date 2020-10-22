use patternfly_yew::*;
use yew::prelude::*;

pub struct Spy {}

impl Component for Spy {
    type Message = ();
    type Properties = ();

    fn create(_props: Self::Properties, _link: ComponentLink<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, _msg: Self::Message) -> bool {
        false
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <h1>{"Device Message Spy"}</h1>
                    </Content>
                </PageSection>
                <PageSection>
                    <div>{"Spy on your things"}</div>
                </PageSection>
            </>
        }
    }
}
