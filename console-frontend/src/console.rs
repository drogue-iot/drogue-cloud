use crate::utils::context::MutableContext;
use crate::{
    backend::{AuthenticatedBackend, BackendInformation},
    components::about::AboutModal,
    examples::{self, Examples},
    pages::{self, apps::ApplicationContext, spy::Spy},
    utils::{context::ContextListener, url_decode},
};
use drogue_cloud_console_common::EndpointInformation;
use patternfly_yew::*;
use std::fmt::Debug;
use std::ops::Deref;
use yew::prelude::*;
use yew_nested_router::prelude::{Switch as RouterSwitch, *};
use yew_oauth2::prelude::*;

#[derive(Target, Debug, Clone, PartialEq, Eq)]
pub enum AppRoute {
    Spy,
    Examples(Examples),
    AccessTokens,
    CurrentToken,
    #[target(rename = "transfer")]
    Ownership(#[target(value)] String),
    Applications(pages::apps::Pages),
    Devices(pages::devices::Pages),
    #[target(index)]
    Overview,
}

#[derive(Clone, Properties, PartialEq)]
pub struct Props {
    pub backend: BackendInformation,
    pub endpoints: EndpointInformation,
    pub on_logout: Callback<()>,
}

pub struct Console {
    app_ctx: MutableContext<ApplicationContext>,

    auth: ContextListener<OAuth2Context>,
    backdropper: ContextListener<Backdropper>,
    router: ContextListener<RouterContext<AppRoute>>,
}

pub enum Msg {
    Logout,
    About,
    CurrentToken,
    SetAppCtx(Box<dyn FnOnce(&mut ApplicationContext)>),
}

impl Component for Console {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            app_ctx: MutableContext::new(Default::default(), ctx.link().callback(Msg::SetAppCtx)),

            auth: ContextListener::unwrap(ctx),
            backdropper: ContextListener::unwrap(ctx),
            router: ContextListener::unwrap(ctx),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Logout => {
                ctx.props().on_logout.emit(());
            }
            Msg::About => self.backdropper.get().open(html! {
                <AboutModal
                    backend={self.backend(ctx.props())}
                />
            }),
            Msg::CurrentToken => self.router.get().push(AppRoute::CurrentToken),
            Msg::SetAppCtx(mutator) => {
                return self.app_ctx.apply(mutator);
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let app = self.app_ctx.context.clone();
        let sidebar = html_nested! (
            <PageSidebar>
                <Nav>
                    <NavList>
                        <NavExpandable title="Home">
                            <NavRouterItem<AppRoute> to={AppRoute::Overview}>{"Overview"}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Applications(pages::apps::Pages::Index)} predicate={AppRoute::is_applications}>{"Applications"}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Devices(pages::devices::Pages::Index{app})} predicate={AppRoute::is_devices}>{"Devices"}</NavRouterItem<AppRoute>>
                        </NavExpandable>
                        <NavExpandable title="Getting started">
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Register)}>{Examples::Register.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Consume)}>{Examples::Consume.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Publish)}>{Examples::Publish.title()}</NavRouterItem<AppRoute>>
                            <NavRouterItem<AppRoute> to={AppRoute::Examples(Examples::Commands)}>{Examples::Commands.title()}</NavRouterItem<AppRoute>>
                        </NavExpandable>
                        <NavExpandable title="Tools">
                            <NavRouterItem<AppRoute> to={AppRoute::Spy}>{"Spy"}</NavRouterItem<AppRoute>>
                        </NavExpandable>
                        <NavExpandable title="API">
                            <NavRouterItem<AppRoute> to={AppRoute::AccessTokens}>{"Access tokens"}</NavRouterItem<AppRoute>>
                            <NavItem to="/api" target="_blank">{"API specification"}<span class="pf-u-ml-sm pf-u-font-size-sm">{Icon::ExternalLinkAlt}</span></NavItem>
                        </NavExpandable>
                    </NavList>
                </Nav>
            </PageSidebar>
        );

        let tools = vec![{
            let (id, name, full_name, account_url, email) =
                if let Some(claims) = self.auth.get().claims() {
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
                        if let Ok(mut paths) = issuer
                            .path_segments_mut()
                            .map_err(|_| anyhow::anyhow!("Failed to modify path"))
                        {
                            paths.push("account");
                        }
                        issuer.to_string()
                    };
                    (
                        id,
                        name,
                        full_name,
                        Some(account_url),
                        claims.email().cloned(),
                    )
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

        let logo = html! (
            <Logo src="/images/logo.png" alt="Drogue IoT" />
        );

        let backend = self.backend(ctx.props());
        let context = self.app_ctx.clone();

        html! (
            <ContextProvider<MutableContext<ApplicationContext>> {context}>
                <Page
                    {logo}
                    {sidebar}
                    tools={Children::new(tools)}
                >
                    <RouterSwitch<AppRoute> render = {move |switch| { match switch {
                        AppRoute::Overview => html!{<pages::Overview
                            endpoints={endpoints.clone()}
                        />},
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
                        AppRoute::CurrentToken => html!{<pages::CurrentToken/>},
                        AppRoute::Examples(example) => html!{
                            <examples::ExamplePage
                                {example}
                                backend={backend.clone()}
                                />
                        },
                    }}}/>
                </Page>
            </ContextProvider<MutableContext<ApplicationContext>>>
        )
    }
}

impl Console {
    fn backend(&self, props: &Props) -> AuthenticatedBackend {
        props.backend.authenticated(
            self.auth
                .get()
                .authentication()
                .cloned()
                .unwrap_or_default(),
        )
    }
}
