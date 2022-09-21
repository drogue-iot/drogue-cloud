use crate::backend::AuthenticatedBackend;
use crate::{
    backend::BackendInformation,
    components::about::AboutModal,
    data::SharedDataBridge,
    examples::{self, Examples},
    pages::{self, apps::ApplicationContext, spy::Spy},
    utils::url_decode,
};
use drogue_cloud_console_common::EndpointInformation;
use patternfly_yew::*;
use std::ops::Deref;
use yew::prelude::*;
use yew_oauth2::prelude::*;
use yew_router::{agent::RouteRequest, prelude::*};

#[derive(Switch, Debug, Clone, PartialEq, Eq)]
pub enum AppRoute {
    #[to = "/spy"]
    Spy,
    #[to = "/examples{*}"]
    Examples(Examples),
    #[to = "/tokens"]
    AccessTokens,
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
    pub backend: BackendInformation,
    pub endpoints: EndpointInformation,
    pub on_logout: Callback<()>,
}

pub struct Console {
    _app_ctx_bridge: SharedDataBridge<ApplicationContext>,
    app_ctx: ApplicationContext,

    auth: ContextValue<OAuth2Context>,
}

pub enum Msg {
    Logout,
    About,
    CurrentToken,
    SetAppCtx(ApplicationContext),
    Auth(OAuth2Context),
}

impl Component for Console {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let app_ctx_bridge = SharedDataBridge::from(ctx.link(), Msg::SetAppCtx);

        let auth = ctx.use_context(Msg::Auth);

        Self {
            _app_ctx_bridge: app_ctx_bridge,
            app_ctx: Default::default(),

            auth,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Logout => {
                ctx.props().on_logout.emit(());
            }
            Msg::About => BackdropDispatcher::default().open(Backdrop {
                content: (html! {
                    <AboutModal
                        backend={self.backend(ctx.props())}
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
            Msg::Auth(auth) => {
                self.auth.set(auth);
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let app = self.app_ctx.clone();
        let sidebar = html_nested! (
            <PageSidebar>
                <Nav>
                    <NavList>
                        <NavRouterExpandable<AppRoute> title="Home">
                            <NavRouterItem<AppRoute> to={AppRoute::Overview}>{"Overview"}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Applications(pages::apps::Pages::Index)}>{"Applications"}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Devices(pages::devices::Pages::Index{app})}>{"Devices"}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="Getting started">
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Register)}>{Examples::Register.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Consume)}>{Examples::Consume.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Publish)}>{Examples::Publish.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Commands)}>{Examples::Commands.title()}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="Tools">
                            <NavRouterItem<AppRoute> to={AppRoute::Spy}>{"Spy"}</NavRouterItem<AppRoute>>
                        </NavRouterExpandable<AppRoute>>
                        <NavRouterExpandable<AppRoute> title="API">
                            <NavRouterItem<AppRoute> to={AppRoute::AccessTokens}>{"Access tokens"}</NavRouterItem<AppRoute>>
                            <NavItem to="/api" target="_blank">{"API specification"}<span class="pf-u-ml-sm pf-u-font-size-sm">{Icon::ExternalLinkAlt}</span></NavItem>
                        </NavRouterExpandable<AppRoute>>
                    </NavList>
                </Nav>
            </PageSidebar>
        );

        let tools = vec![{
            let (id, name, full_name, account_url, email) =
                if let Some(claims) = self.auth.as_ref().and_then(|auth| auth.claims()) {
                    let id = claims.subject().to_string();
                    let name = claims
                        .preferred_username()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let full_name = claims
                        .name()
                        .and_then(|name| name.get(None))
                        .map(|s| s.to_string());
                    let account_url = {
                        let mut issuer = claims.issuer().url().clone();
                        issuer
                            .path_segments_mut()
                            .map_err(|_| anyhow::anyhow!("Failed to modify path"))
                            .ok()
                            .map(|mut paths| {
                                paths.push("account");
                            });
                        issuer.to_string()
                    };
                    (id, name, full_name, Some(account_url), claims.email())
                } else {
                    (String::new(), String::new(), None, None, None)
                };

            let src = email
                .map(|email| md5::compute(email.as_bytes()))
                .map(|hash| format!("https://www.gravatar.com/avatar/{:x}?D=mp", hash))
                .unwrap_or_else(|| "/assets/images/img_avatar.svg".into());

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

            items.push((html_nested! (<Divider/>)).into());

            // links
            items.push({
                let mut items = Vec::new();
                items.push(html_nested!{<DropdownItem onclick={ctx.link().callback(|_|Msg::CurrentToken)}>{"Current Token"}</DropdownItem>});
                if let Some(account_url) = account_url {
                    items.push(html_nested! (
                        <DropdownItem target="_blank" href={account_url}>{"Account"} <span class="pf-u-pl-sm">{Icon::ExternalLinkAlt}</span></DropdownItem>
                    ));
                }
                items.push(html_nested!{<DropdownItem onclick={ctx.link().callback(|_|Msg::Logout)}>{"Logout"}</DropdownItem>});

                (html_nested!{<DropdownItemGroup>{items}</DropdownItemGroup>}).into()
            });

            // render

            let user_toggle = html! (<UserToggle name={full_name.unwrap_or(name)} src={src} />);
            html! (
                <>
                <AppLauncher
                    position={Position::Right}
                    toggle={Icon::QuestionCircle}
                    >
                    <AppLauncherItem external=true href="https://book.drogue.io">{"Documentation"}</AppLauncherItem>
                    <Divider/>
                    <AppLauncherItem onclick={ctx.link().callback(|_|Msg::About)}>{"About"}</AppLauncherItem>
                </AppLauncher>
                <Dropdown
                    id="user-dropdown"
                    plain=true
                    position={Position::Right}
                    toggle_style="display: flex;"
                    toggle={user_toggle}
                    >
                {items}
                </Dropdown>
                </>
            )
        }];

        let endpoints = ctx.props().endpoints.clone();

        let logo = html_nested! (
            <Logo src="/images/logo.png" alt="Drogue IoT" />
        );

        let backend = self.backend(ctx.props());

        html! (
            <Page
                {logo}
                {sidebar}
                tools={Children::new(tools)}
                >
                    <Router<AppRoute, ()>
                            redirect = {Router::redirect(|_|AppRoute::Overview)}
                            render = {Router::render(move |switch: AppRoute| {
                                match switch {
                                    AppRoute::Overview => html!{<pages::Overview
                                        endpoints={endpoints.clone()}/>},
                                    AppRoute::Applications(pages::apps::Pages::Index) => html!{<pages::apps::Index
                                        backend={backend.clone()}
                                    />},
                                    AppRoute::Applications(pages::apps::Pages::Details{name, details}) => html!{<pages::apps::Details
                                        backend={backend.clone()}
                                        endpoints={endpoints.clone()}
                                        name={url_decode(&name)}
                                        details={details}
                                    />},
                                    AppRoute::Ownership(id) => html!{<pages::apps::ownership::Ownership
                                        backend={backend.clone()}
                                        name={url_decode(&id)}
                                    />},
                                    AppRoute::Devices(pages::devices::Pages::Index{app}) => html!{<pages::devices::Index
                                        app={app.to_string()}
                                        backend={backend.clone()}
                                    />},
                                    AppRoute::Devices(pages::devices::Pages::Details{app, name, details}) => html!{<pages::devices::Details
                                        backend={backend.clone()}
                                        endpoints={endpoints.clone()}
                                        app={url_decode(&app.to_string())}
                                        name={url_decode(&name)}
                                        details={details}
                                    />},
                                    AppRoute::Spy => html!{<Spy
                                        backend={backend.deref().clone()}
                                        endpoints={endpoints.clone()}
                                    />},
                                    AppRoute::AccessTokens => html!{<pages::AccessTokens
                                        backend={backend.clone()}
                                    />},
                                    AppRoute::CurrentToken => html!{<pages::CurrentToken
                                    />},
                                    AppRoute::Examples(example) => html!{
                                        <examples::ExamplePage
                                            {example}
                                            backend={backend.clone()}
                                            />
                                    },
                                }
                            })}
                        />
            </Page>
        )
    }
}

impl Console {
    fn backend(&self, props: &Props) -> AuthenticatedBackend {
        props.backend.authenticated(
            self.auth
                .get()
                .and_then(|auth| auth.authentication().cloned())
                .unwrap_or_default(),
        )
    }
}
