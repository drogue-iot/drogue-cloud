use crate::{backend::Token, html_prop};
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, PartialEq, Eq, Properties)]
pub struct Props {
    pub token: Token,
}

pub struct CurrentToken {}

impl Component for CurrentToken {
    type Message = ();
    type Properties = Props;

    fn create(_: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let token = ctx
            .props()
            .token
            .refresh_token
            .as_ref()
            .cloned()
            .unwrap_or_default();

        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light} limit_width=true>
                    <Content>
                        <Title>{"Current API token"}</Title>
                    </Content>
                </PageSection>
                <PageSection>
                    <Card
                        title={html_prop!({"Current API refresh token"})}
                        >
                        <Clipboard
                            readonly=true
                            code=true
                            variant={ClipboardVariant::Expandable}
                            value={token}
                        />
                    </Card>
                </PageSection>
            </>
        };
    }
}
