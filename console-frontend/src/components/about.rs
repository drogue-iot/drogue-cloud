use crate::backend::{
    ApiResponse, AuthenticatedBackend, Json, JsonHandlerScopeExt, Nothing, RequestHandle,
};
use crate::error::{error, ErrorNotification, ErrorNotifier};
use crate::utils::context::ContextListener;
use drogue_cloud_service_api::version::Version;
use http::Method;
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, Debug, PartialEq, Properties)]
pub struct Props {
    pub backend: AuthenticatedBackend,
}

pub enum Msg {
    FetchInfo,
    Info(Version),
    Error(ErrorNotification),
}

pub struct AboutModal {
    toaster: ContextListener<Toaster>,
    info: Option<Version>,
    task: Option<RequestHandle>,
}

impl Component for AboutModal {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::FetchInfo);
        Self {
            toaster: ContextListener::unwrap(ctx),
            info: None,
            task: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::FetchInfo => match self.fetch_info(ctx) {
                Ok(task) => self.task = Some(task),
                Err(err) => error(&self.toaster.get(), "Failed to fetch information", err),
            },
            Msg::Info(info) => self.info = Some(info),
            Msg::Error(msg) => {
                msg.toast(&self.toaster.get());
            }
        }
        true
    }

    fn view(&self, _: &Context<Self>) -> Html {
        html! {
            <Bullseye plain=true>
                <About
                    brand_src="/images/logo.svg"
                    title="Drogue IoT Cloud"
                    hero_style=r#"--pf-c-about-modal-box__hero--sm--BackgroundImage: url("/images/about.jpg"); --pf-c-about-modal-box__hero--sm--BackgroundPosition: bottom right; --pf-c-about-modal-box__hero--sm--BackgroundSize: contain; background-attachment: local;"#
                >
                    <Content>
                        if let Some(info) = &self.info {
                            <dl style="width: 100%">
                                <dt>{"Version"}</dt><dd>{&info.version}</dd>
                            </dl>
                        }
                    </Content>
                </About>
            </Bullseye>
        }
    }
}

impl AboutModal {
    fn fetch_info(&self, ctx: &Context<Self>) -> anyhow::Result<RequestHandle> {
        Ok(ctx.props().backend.request(
            Method::GET,
            "/.well-known/drogue-version",
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<Json<Version>, _>(|response| match response {
                ApiResponse::Success(info, _) => Msg::Info(info),
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to load information")),
            }),
        )?)
    }
}
