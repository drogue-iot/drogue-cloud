use crate::html_prop;
use patternfly_yew::*;
use yew::prelude::*;
use yew_oauth2::prelude::*;

pub enum Msg {
    Auth(OAuth2Context),
}

pub struct CurrentToken {
    auth: ContextValue<OAuth2Context>,
}

impl Component for CurrentToken {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let auth = ctx
            .link()
            .context::<OAuth2Context>(ctx.link().callback(Msg::Auth))
            .into();

        Self { auth }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::Auth(auth) => {
                self.auth.set(auth);
            }
        }
        true
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let token = self
            .auth
            .get()
            .and_then(|auth| match auth {
                OAuth2Context::Authenticated(Authentication { refresh_token, .. }) => {
                    refresh_token.clone()
                }
                _ => None,
            })
            .unwrap_or_default();

        html! (
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
        )
    }
}
