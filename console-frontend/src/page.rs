use crate::{
    backend::Token,
    examples::{self, Examples},
    index::Index,
    spy::Spy,
};
use patternfly_yew::*;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Switch, Debug, Clone, PartialEq)]
pub enum AppRoute {
    #[to = "/spy"]
    Spy,
    #[to = "/examples{*:rest}"]
    Examples(Examples),
    #[to = "/"]
    Index,
}

#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub token: Token,
    pub on_logout: Callback<()>,
}

pub struct AppPage {
    props: Props,
    link: ComponentLink<Self>,
}

pub enum Msg {
    Logout,
}

impl Component for AppPage {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self { props, link }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Logout => {
                self.props.on_logout.emit(());
            }
        }
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
        let sidebar = html_nested! {
            <PageSidebar>
                <Nav>
                    <NavList>
                        <NavRouterItem<AppRoute> to=AppRoute::Index>{"Home"}</NavRouterItem<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="Getting started">
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Register)>{Examples::Register.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Consume)>{Examples::Consume.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Publish)>{Examples::Publish.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Commands)>{Examples::Commands.title()}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="Tools">
                            <NavRouterItem<AppRoute> to=AppRoute::Spy>{"Spy"}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavItem to="/api" target="_blank">{"API "}<span class="pf-u-ml-sm pf-u-font-size-sm">{Icon::ExternalLinkAlt}</span></NavItem>
                    </NavList>
                </Nav>
            </PageSidebar>
        };

        let tools = vec![{
            let (name, full_name, account_url) =
                if let Some(userinfo) = self.props.token.userinfo.as_ref() {
                    let name = userinfo.name.clone();
                    let full_name = userinfo.full_name.as_ref().cloned();
                    (name, full_name, userinfo.account_url.as_ref().cloned())
                } else {
                    (String::new(), None, None)
                };

            let src = self
                .props
                .token
                .userinfo
                .as_ref()
                .and_then(|user| {
                    if user.email_verified {
                        user.email.as_ref().cloned()
                    } else {
                        None
                    }
                })
                .map(|email| md5::compute(email.as_bytes()))
                .map(|hash| format!("https://www.gravatar.com/avatar/{:x}?D=mp", hash))
                .unwrap_or_else(|| "/images/img_avatar.svg".into());

            // gather items

            let mut items = Vec::<DropdownChildVariant>::new();

            // panel
            items.push({
                let mut texts = Vec::new();

                texts.push(html_nested! {
                    <DropdownItemText>
                    <dl>
                        <dt class="pf-u-font-size-xs">{"Username:"}</dt>
                        <dd>{&name}</dd>
                    </dl>
                    </DropdownItemText>
                });

                (html_nested! {<DropdownItemGroup>{texts}</DropdownItemGroup>}).into()
            });

            items.push((html_nested! {<Divider/>}).into());

            // links
            items.push({
                let mut items = Vec::new();
                if let Some(account_url) = account_url {
                    items.push(html_nested! {
                        <DropdownItem target="_blank" href=account_url>{"Account"} <span class="pf-u-pl-sm">{Icon::ExternalLinkAlt}</span></DropdownItem>
                    });
                }
                items.push(html_nested!{<DropdownItem onclick=self.link.callback(|_|Msg::Logout)>{"Logout"}</DropdownItem>});

                (html_nested!{<DropdownItemGroup>{items}</DropdownItemGroup>}).into()
            });

            // render

            html! {
                <Dropdown
                    plain=true
                    toggle_style="display: flex;"
                    toggle=html!{<UserToggle name=full_name.unwrap_or(name) src=src />}
                    >
                {items}
                </Dropdown>
            }
        }];

        return html! {
            <Page
                logo={html_nested!{
                    <Logo src="/images/logo.png" alt="Drogue IoT" />
                }}
                sidebar=sidebar
                tools=Children::new(tools)
                >
                    <Router<AppRoute, ()>
                            redirect = Router::redirect(|_|AppRoute::Index)
                            render = Router::render(|switch: AppRoute| {
                                match switch {
                                    AppRoute::Spy => html!{<Spy/>},
                                    AppRoute::Index => html!{<Index/>},

                                    AppRoute::Examples(example) => html!{
                                        <examples::ExamplePage example=example/>
                                    },
                                }
                            })
                        />
            </Page>
        };
    }
}
