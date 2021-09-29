use crate::utils::url_encode;
use crate::{
    backend::Backend,
    error::error,
    page::AppRoute,
    pages::{
        apps::{DetailsSection, Pages},
        HasReadyState,
    },
};
use drogue_client::registry::v1::Application;
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*};
use yew_router::{agent::RouteRequest, prelude::*};

#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationEntry {
    pub app: Application,
    pub on_overview: Callback<()>,
    pub on_delete: Callback<()>,
}

impl TableRenderer for ApplicationEntry {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => html! {
                <a onclick=self.on_overview.clone().reform(|_|())>{self.app.metadata.name.clone()}</a>
            },
            1 => self.app.render_state(),
            2 => self
                .app
                .metadata
                .creation_timestamp
                .format("%e %b %Y, %k:%M")
                .into(),
            _ => html! {},
        }
    }

    fn actions(&self) -> Vec<DropdownChildVariant> {
        vec![html_nested! {
        <DropdownItem
            onclick=self.on_delete.clone()
        >
            {"Delete"}
        </DropdownItem>}
        .into()]
    }
}

#[derive(Clone, PartialEq, Eq, Properties)]
pub struct Props {
    pub backend: Backend,
}

pub enum Msg {
    Load,
    SetData(Vec<ApplicationEntry>),
    Error(String),

    ShowOverview(String),
    Delete(String),
}

pub struct Index {
    props: Props,
    link: ComponentLink<Self>,
    entries: Vec<ApplicationEntry>,

    fetch_task: Option<FetchTask>,
}

impl Component for Index {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::Load);
        Self {
            props,
            link,
            entries: Vec::new(),
            fetch_task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::Load => match self.load() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::SetData(keys) => {
                self.entries = keys;
                self.fetch_task = None;
            }
            Msg::Error(msg) => {
                error("Error", msg);
            }
            Msg::ShowOverview(name) => RouteAgentDispatcher::<()>::new().send(
                RouteRequest::ChangeRoute(Route::from(AppRoute::Applications(Pages::Details {
                    name,
                    details: DetailsSection::Overview,
                }))),
            ),
            Msg::Delete(name) => match self.delete(name) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to delete", err),
            },
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
                        <Title>{"Applications"}</Title>
                    </Content>
                </PageSection>
                <PageSection>
                    <Table<SimpleTableModel<ApplicationEntry>>
                        entries=SimpleTableModel::from(self.entries.clone())
                        header={html_nested!{
                            <TableHeader>
                                <TableColumn label="Name"/>
                                <TableColumn label="Status"/>
                                <TableColumn label="Created"/>
                            </TableHeader>
                        }}
                        >
                    </Table<SimpleTableModel<ApplicationEntry>>>
                </PageSection>
            </>
        };
    }
}

impl Index {
    fn load(&self) -> Result<FetchTask, anyhow::Error> {
        let link = self.link.clone();

        self.props.backend.info.request(
            Method::GET,
            "/api/registry/v1alpha1/apps",
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<Vec<Application>, anyhow::Error>>>| {
                    match response.into_body().0 {
                        Ok(entries) => {
                            let link = link.clone();
                            let entries = entries
                                .into_iter()
                                .map(move |app| {
                                    let name = app.metadata.name.clone();
                                    let name_copy = app.metadata.name.clone();

                                    let on_overview =
                                        link.callback_once(move |_| Msg::ShowOverview(name));

                                    ApplicationEntry {
                                        app,
                                        on_overview,
                                        on_delete: link
                                            .callback_once(move |_| Msg::Delete(name_copy)),
                                    }
                                })
                                .collect();
                            Msg::SetData(entries)
                        }
                        Err(err) => Msg::Error(err.to_string()),
                    }
                },
            ),
        )
    }

    fn delete(&self, name: String) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::DELETE,
            format!("/api/registry/v1alpha1/apps/{}", url_encode(name)),
            Nothing,
            vec![],
            self.link
                .callback(move |response: Response<Text>| match response.status() {
                    StatusCode::NO_CONTENT => Msg::Load,
                    status => Msg::Error(format!("Cannot delete application : {}", status)),
                }),
        )
    }
}
