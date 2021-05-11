use crate::backend::Token;
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, PartialEq, Eq, Properties)]
pub struct Props {
    pub token: Token,
}

pub struct CurrentToken {
    props: Props,
}

impl Component for CurrentToken {
    type Message = ();
    type Properties = Props;

    fn create(props: Self::Properties, _link: ComponentLink<Self>) -> Self {
        Self { props }
    }

    fn update(&mut self, _msg: Self::Message) -> ShouldRender {
        true
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
        let token = self.props.token.refresh_token.as_deref().unwrap_or("");

        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <Title>{"Current API token"}</Title>
                    </Content>
                </PageSection>
                <PageSection>
                    <Card
                        title={html!{"Current API refresh token"}}
                        >
                        <Clipboard
                            readonly=true
                            code=true
                            variant=ClipboardVariant::Expandable
                            value=token>
                        </Clipboard>
                    </Card>
                </PageSection>
            </>
        };
    }
}
