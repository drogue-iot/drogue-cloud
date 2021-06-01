use crate::{backend::BackendInformation, error::error};
use drogue_cloud_service_api::version::DrogueVersion;
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*};

#[derive(Clone, Debug, PartialEq, Eq, Properties)]
pub struct Props {
    pub backend: BackendInformation,
}

pub enum Msg {
    FetchInfo,
    Info(DrogueVersion),
    Error(String),
}

pub struct AboutModal {
    props: Props,
    link: ComponentLink<Self>,
    info: Option<DrogueVersion>,
    task: Option<FetchTask>,
}

impl Component for AboutModal {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::FetchInfo);
        Self {
            props,
            link,
            info: None,
            task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::FetchInfo => match self.fetch_info() {
                Ok(task) => self.task = Some(task),
                Err(err) => error("Failed to fetch information", err),
            },
            Msg::Info(info) => self.info = Some(info),
            Msg::Error(err) => error("Failed to fetch information", err),
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
        return html! {
            <Bullseye plain=true>
                <About
                    brand_src="/images/logo.svg"
                    title="Drogue IoT Cloud"
                >
                <Content>
                    { if let Some(info) = &self.info {html!{
                        <dl style="width: 100%">
                            <dt>{"Version"}</dt><dd>{&info.version}</dd>
                        </dl>
                    }} else { html!{}}}
                </Content>
                </About>
            </Bullseye>
        };
    }
}

impl AboutModal {
    fn fetch_info(&self) -> anyhow::Result<FetchTask> {
        self.props.backend.request(
            Method::GET,
            "/.well-known/drogue-version",
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<DrogueVersion, anyhow::Error>>>| match response
                    .into_body()
                    .0
                {
                    Ok(info) => Msg::Info(info),
                    Err(err) => Msg::Error(err.to_string()),
                },
            ),
        )
    }
}
