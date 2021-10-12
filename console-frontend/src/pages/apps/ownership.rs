use crate::utils::url_encode;
use crate::{
    backend::Backend,
    error::error,
    page::AppRoute,
    pages::apps::{DetailsSection, Pages},
};
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*};
use yew_router::{agent::RouteRequest, prelude::*};

#[derive(Clone, PartialEq, Eq, Properties)]
pub struct Props {
    pub backend: Backend,
    pub name: String,
}

pub enum Msg {
    Load,
    Error(String),
    Done,
}

pub struct Ownership {
    props: Props,
    link: ComponentLink<Self>,

    fetch_task: Option<FetchTask>,
}

impl Component for Ownership {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::Load);
        Self {
            props,
            link,
            fetch_task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Load => match self.load() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::Error(msg) => {
                error("Error", msg);
            }
            Msg::Done => RouteAgentDispatcher::<()>::new().send(RouteRequest::ChangeRoute(
                Route::from(AppRoute::Applications(Pages::Details {
                    name: self.props.name.clone(),
                    details: DetailsSection::Overview,
                })),
            )),
        };
        true
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        true
    }

    fn view(&self) -> Html {
        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light limit_width=true>
                    <Content>
                        <Title>{"Application ownership transfer"}</Title>
                    </Content>
                </PageSection>
            </>
        };
    }
}

impl Ownership {
    fn load(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::PUT,
            url_encode(
            format!("/api/admin/v1alpha1/apps/{}/accept-ownership", self.props.name)),
            Nothing,
            vec![],
            self.link.callback( move |response: Response<Text>| match response.status() {
                    StatusCode::NO_CONTENT => {
                        ToastDispatcher::default().toast(Toast {
                            title: "Success !".into(),
                            body: html! {<>
                                <Content>
                                <p>{"Ownership transfer completed. You are now the owner of this app."}</p>
                                </Content>
                            </>},
                            r#type: Type::Success,
                            ..Default::default()
                        });
                        Msg::Done
                    }
                    status => Msg::Error(format!(
                        "Failed to submit: Code {}. {}",
                        status,
                        response
                            .body()
                            .as_ref()
                            .unwrap_or(&"Unknown error.".to_string())
                    )),
                },
            ),
        )
    }
}
