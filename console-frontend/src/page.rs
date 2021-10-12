use crate::{
    backend::{Backend, Token},
    components::about::AboutModal,
    data::SharedDataBridge,
    examples::{self, Examples},
    pages::{self, apps::ApplicationContext},
    spy::Spy,
    utils::url_decode,
};
use drogue_cloud_console_common::EndpointInformation;
use patternfly_yew::*;
use yew::prelude::*;
use yew_router::{agent::RouteRequest, prelude::*};

#[derive(Switch, Debug, Clone, PartialEq, Eq)]
pub enum AppRoute {
    #[to = "/spy"]
    Spy,
    #[to = "/examples{*}"]
    Examples(Examples),
    #[to = "/keys"]
    ApiKeys,
    #[to = "/token"]
    CurrentToken,
    #[to = "/transfer/{name}"]
    Ownership(String),
    #[to = "/apps{*}"]
    Applications(pages::apps::Pages),
    #[to = "/devices{*}"]
    Devices(pages::devices::Pages),
    #[to = "/!"]
    Overview,
}

#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub backend: Backend,
    pub endpoints: EndpointInformation,
    pub token: Token,
    pub on_logout: Callback<()>,
}

pub struct AppPage {
    props: Props,
    link: ComponentLink<Self>,

    _app_ctx_bridge: SharedDataBridge<ApplicationContext>,
    app_ctx: ApplicationContext,
}

pub enum Msg {
    Logout,
    About,
    CurrentToken,
    SetAppCtx(ApplicationContext),
}

impl Component for AppPage {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let app_ctx_bridge = SharedDataBridge::from(&link, Msg::SetAppCtx);

        Self {
            props,
            link,
            _app_ctx_bridge: app_ctx_bridge,
            app_ctx: Default::default(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Logout => {
                self.props.on_logout.emit(());
            }
            Msg::About => BackdropDispatcher::default().open(Backdrop {
                content: (html! {
                    <AboutModal
                        backend=self.props.backend.info.clone()
                        />
                }),
            }),
            Msg::CurrentToken => RouteAgentDispatcher::<()>::new().send(RouteRequest::ChangeRoute(
                Route::from(AppRoute::CurrentToken),
            )),
            Msg::SetAppCtx(ctx) => {
                return if self.app_ctx != ctx {
                    self.app_ctx = ctx;
                    true
                } else {
                    false
                };
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
        let app = self.app_ctx.clone();
        let sidebar = html_nested! {
            <PageSidebar>
                <Nav>
                    <NavList>
                        <NavRouterExpandable<AppRoute> title="Home">
                            <NavRouterItem<AppRoute> to=AppRoute::Overview>{"Overview"}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Applications(pages::apps::Pages::Index)>{"Applications"}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Devices(pages::devices::Pages::Index{app})>{"Devices"}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="Getting started">
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Register)>{Examples::Register.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Consume)>{Examples::Consume.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Publish)>{Examples::Publish.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to=AppRoute::Examples(Examples::Commands)>{Examples::Commands.title()}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="Tools">
                            <NavRouterItem<AppRoute> to=AppRoute::Spy>{"Spy"}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="API">
                            <NavRouterItem<AppRoute> to=AppRoute::ApiKeys>{"Access keys"}</NavRouterItem<AppRoute>>
                            <NavItem to="/api" target="_blank">{"API specification"}<span class="pf-u-ml-sm pf-u-font-size-sm">{Icon::ExternalLinkAlt}</span></NavItem>
                        </NavRouterExpandable<AppRoute>>
                    </NavList>
                </Nav>
            </PageSidebar>
        };

        let tools = vec![{
            let (id, name, full_name, account_url) =
                if let Some(userinfo) = self.props.token.userinfo.as_ref() {
                    let id = userinfo.id.clone();
                    let name = userinfo.name.clone();
                    let full_name = userinfo.full_name.as_ref().cloned();
                    (id, name, full_name, userinfo.account_url.as_ref().cloned())
                } else {
                    (String::new(), String::new(), None, None)
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
                    <dl>
                        <dt class="pf-u-font-size-xs">{"ID:"}</dt>
                        <dd>{&id}</dd>
                    </dl>
                    </DropdownItemText>
                });

                (html_nested! {<DropdownItemGroup>{texts}</DropdownItemGroup>}).into()
            });

            items.push((html_nested! {<Divider/>}).into());

            // links
            items.push({
                let mut items = Vec::new();
                items.push(html_nested!{<DropdownItem onclick=self.link.callback(|_|Msg::CurrentToken)>{"Current Token"}</DropdownItem>});
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
                <>
                <AppLauncher
                    position=Position::Right
                    toggle=html!{Icon::QuestionCircle}
                    >
                    <AppLauncherItem external=true href="https://book.drogue.io">{"Documentation"}</AppLauncherItem>
                    <Divider/>
                    <AppLauncherItem onclick=self.link.callback(|_|Msg::About)>{"About"}</AppLauncherItem>
                </AppLauncher>
                <Dropdown
                    id="user-dropdown"
                    plain=true
                    position=Position::Right
                    toggle_style="display: flex;"
                    toggle=html!{<UserToggle name=full_name.unwrap_or(name) src=src />}
                    >
                {items}
                </Dropdown>
                </>
            }
        }];

        let endpoints = self.props.endpoints.clone();
        let backend = self.props.backend.clone();
        let token = self.props.token.clone();

        return html! {
            <Page
                logo={html_nested!{
                    <Logo src="/images/logo.png" alt="Drogue IoT" />
                }}
                sidebar=sidebar
                tools=Children::new(tools)
                >
                    <Router<AppRoute, ()>
                            redirect = Router::redirect(|_|AppRoute::Overview)
                            render = Router::render(move |switch: AppRoute| {
                                match switch {
                                    AppRoute::Overview => html!{<pages::Overview endpoints=endpoints.clone()/>},
                                    AppRoute::Applications(pages::apps::Pages::Index) => html!{<pages::apps::Index
                                        backend=backend.clone()
                                    />},
                                    AppRoute::Applications(pages::apps::Pages::Details{name, details}) => html!{<pages::apps::Details
                                        backend=backend.clone()
                                        token=token.clone()
                                        endpoints=endpoints.clone()
                                        name=url_decode(&name)
                                        details=details
                                    />},
                                    AppRoute::Ownership(id) => html!{<pages::apps::ownership::Ownership
                                        backend=backend.clone()
                                        name=url_decode(&id)
                                    />},
                                    AppRoute::Devices(pages::devices::Pages::Index{app}) => html!{<pages::devices::Index
                                        app=app.to_string()
                                        backend=backend.clone()
                                    />},
                                    AppRoute::Devices(pages::devices::Pages::Details{app, name, details}) => html!{<pages::devices::Details
                                        backend=backend.clone()
                                        app=url_decode(&app.to_string())
                                        name=url_decode(&name)
                                        details=details
                                    />},
                                    AppRoute::Spy => html!{<Spy
                                        backend=backend.clone()
                                        token=token.clone()
                                        endpoints=endpoints.clone()
                                    />},
                                    AppRoute::ApiKeys => html!{<pages::ApiKeys
                                        backend=backend.clone()
                                    />},
                                    AppRoute::CurrentToken => html!{<pages::CurrentToken
                                        token=token.clone()
                                    />},

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
