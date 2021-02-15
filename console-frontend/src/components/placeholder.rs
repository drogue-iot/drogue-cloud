use crate::backend::Backend;
use patternfly_yew::*;
use yew::prelude::*;

pub struct Placeholder {
    link: ComponentLink<Self>,
}

pub enum Msg {
    Login,
}

impl Component for Placeholder {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self { link }
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
        let footer = html! {<>
            <p>{"This is the login to the Drogue IoT console."}</p>
            <List r#type=ListType::Inline>
                <a href="https://blog.drogue.io" target="_blank">{"Learn more"}</a>
            </List>
        </>};

        let header = Children::new(vec![header]);
        let footer = Children::new(vec![footer]);

        let onclick = self.link.callback(|_| Msg::Login);

        html! {
            <>
                <Background filter="contrast(65%) brightness(80%)"/>
                <Login
                    header=header
                    footer=footer
                    >
                    <LoginMain>
                        <LoginMainHeader
                            title=html_nested!{<Title size=Size::XXLarge>{"Login to the console"}</Title>}
                            description="Log in to the Drogue IoT console using the single sign-on (SSO) mechanism."
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

                    </LoginMain>
                </Login>
            </>
        }
    }
}
