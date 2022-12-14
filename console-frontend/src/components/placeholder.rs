use crate::{
    backend::BackendInformation,
    console::AppRoute,
    utils::{context::ContextListener, ToHtml},
};
use patternfly_yew::*;
use std::collections::HashMap;
use yew::prelude::*;
use yew_nested_router::prelude::{Switch as RouterSwitch, *};
use yew_oauth2::{agent::OpenIdClient, openid, prelude::*};

#[derive(Clone, Debug, Properties, PartialEq, Eq)]
pub struct Props {
    pub info: BackendInformation,
}

pub struct Placeholder {
    agent: ContextListener<context::Agent<OpenIdClient>>,
}

impl Component for Placeholder {
    type Message = ();
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            agent: ContextListener::new(ctx),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let login = self.render_login(ctx);

        html!(
            <RouterSwitch<AppRoute>
                default = { login.clone() }
                render = {move |switch| {
                    match switch {
                        AppRoute::Overview => { login.clone() },
                        _ => {
                            html!(<openid::RouterRedirect<AppRoute> logout={AppRoute::Overview} />)
                        }
                    }
                }}
            />
        )
    }
}

impl Placeholder {
    fn default_login_note() -> Html {
        html! (
            <>
                <p>{"This is the login to the Drogue IoT console."}</p>
                <List r#type={ListType::Inline}>
                    <a href="https://drogue.io" target="_blank">{"Learn more"}</a>
                </List>
            </>
        )
    }

    fn render_login(&self, ctx: &Context<Self>) -> Html {
        let header = html! {<> <img src="/images/logo.svg" /> </>};

        let footer = ctx
            .props()
            .info
            .login_note
            .as_ref()
            .map(|note| note.to_html())
            .unwrap_or_else(Self::default_login_note);

        let onclick = ctx.link().callback(|_| Msg::Login);
        let title = html_nested! {<Title size={Size::XXLarge}>{"Login to the console"}</Title>};

        html! (
            <>
                <Background filter="contrast(65%) brightness(80%)"/>
                <Login
                    header={Children::new(vec![header])}
                    footer={Children::new(vec![footer])}
                    >
                    <LoginMain>
                        <LoginMainHeader
                            title={title}
                            description="Log in to the Drogue IoT console using single sign-on (SSO)."
                            />
                        <LoginMainBody>
                            <Form>
                                <ActionGroup>
                                    <Button
                                        label="Log in via SSO"
                                        variant={Variant::Primary}
                                        onclick={onclick}
                                        block=true
                                        />
                                </ActionGroup>
                            </Form>
                        </LoginMainBody>
                        { self.render_main_footer(ctx) }
                    </LoginMain>
                </Login>
            </>
        )
    }

    fn render_main_footer(&self, ctx: &Context<Self>) -> Html {
        if ctx.props().info.idps.is_empty() && ctx.props().info.footer_band.is_empty() {
            return html! {};
        }

        let band = ctx
            .props()
            .info
            .footer_band
            .iter()
            .map(|item| item.to_html())
            .collect();
        let band: Children = Children::new(band);

        html! (
            <LoginMainFooter
                    links={self.idp_links(ctx)}
                    band={band}
                >
            </LoginMainFooter>
        )
    }

    fn idp_links(&self, ctx: &Context<Self>) -> ChildrenWithProps<LoginMainFooterLink> {
        let links = ctx
            .props()
            .info
            .idps
            .iter()
            .map(|idp| {
                let label = idp.label.clone().unwrap_or_default();
                let (href, onclick) = match &idp.href {
                    Some(href) => (Some(href.clone()), None),
                    None => {
                        let id = idp.id.clone();
                        (
                            None,
                            Some(Callback::from(move |_: MouseEvent| {
                                let _ = self.agent.start_login_opts(LoginOptions {
                                    query: {
                                        let mut q = HashMap::new();
                                        q.insert("kc_idp_hint".to_string(), id.clone());
                                        q
                                    },
                                });
                            })),
                        )
                    }
                };

                html_nested! {
                    <LoginMainFooterLink {href} {onclick} {label}>
                        { idp.icon_html.to_html() }
                    </LoginMainFooterLink>
                }
            })
            .collect();

        ChildrenWithProps::new(links)
    }
}
