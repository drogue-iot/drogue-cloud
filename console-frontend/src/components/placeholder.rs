use crate::{
    backend::{Backend, BackendInformation},
    utils::ToHtml,
};
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, Debug, Properties)]
pub struct Props {
    pub info: BackendInformation,
}

pub struct Placeholder {
    props: Props,
    link: ComponentLink<Self>,
}

pub enum Msg {
    Login,
}

impl Component for Placeholder {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self { props, link }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Login => {
                Backend::reauthenticate().ok();
            }
        }
        true
    }

    fn change(&mut self, _: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        let header = html! {<> <img src="/images/logo.svg" /> </>};

        let footer = self
            .props
            .info
            .login_note
            .as_ref()
            .map(|note| note.to_html())
            .unwrap_or_else(Self::default_login_note);

        let onclick = self.link.callback(|_| Msg::Login);

        html! {
            <>
                <Background filter="contrast(65%) brightness(80%)"/>
                <Login
                    header=Children::new(vec![header])
                    footer=Children::new(vec![footer])
                    >
                    <LoginMain>
                        <LoginMainHeader
                            title=html_nested!{<Title size=Size::XXLarge>{"Login to the console"}</Title>}
                            description="Log in to the Drogue IoT console using single sign-on (SSO)."
                            />
                        <LoginMainBody>
                            <Form>
                                <ActionGroup>
                                    <Button
                                        label="Log In via SSO"
                                        variant=Variant::Primary
                                        onclick=onclick
                                        />
                                </ActionGroup>
                            </Form>
                        </LoginMainBody>
                        { self.render_main_footer() }
                    </LoginMain>
                </Login>
            </>
        }
    }
}

impl Placeholder {
    fn default_login_note() -> Html {
        return html! {<>
            <p>{"This is the login to the Drogue IoT console."}</p>
            <List r#type=ListType::Inline>
                <a href="https://drogue.io" target="_blank">{"Learn more"}</a>
            </List>
        </>};
    }

    fn render_main_footer(&self) -> Html {
        if self.props.info.idps.is_empty() && self.props.info.footer_band.is_empty() {
            return html! {};
        }

        let band = self
            .props
            .info
            .footer_band
            .iter()
            .map(|item| item.to_html())
            .collect();
        let band: Children = Children::new(band);

        return html! {
            <LoginMainFooter
                    links=self.idp_links()
                    band=band
                >
            </LoginMainFooter>
        };
    }

    fn idp_links(&self) -> ChildrenWithProps<LoginMainFooterLink> {
        let links = self
            .props
            .info
            .idps
            .iter()
            .map(|idp| {
                // use the provided href ...
                let href = idp.href.clone().unwrap_or_else(|| {
                    // ... or create a default one using the idp hint
                    let mut href = self.props.info.url("/api/console/v1alpha1/ui/login");
                    href.query_pairs_mut().append_pair("kc_idp_hint", &idp.id);
                    href.to_string()
                });

                let label = idp.label.clone().unwrap_or_default();
                html_nested! {
                    <LoginMainFooterLink href=href label=label>
                        { idp.icon_html.to_html() }
                    </LoginMainFooterLink>
                }
            })
            .collect();

        ChildrenWithProps::new(links)
    }
}
