use crate::backend::{ApiResponse, Json, JsonHandlerScopeExt, Nothing, RequestHandle};
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::url_encode;
use crate::{
    backend::Backend,
    error::error,
    page::AppRoute,
    pages::{
        apps::{CreateDialog, DetailsSection, Pages},
        HasReadyState,
    },
};
use drogue_client::registry::v1::Application;
use http::{Method, StatusCode};
use patternfly_yew::*;
use yew::prelude::*;
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
                <a onclick={self.on_overview.clone().reform(|_|())}>{self.app.metadata.name.clone()}</a>
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
            onclick={self.on_delete.clone()}
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
    Error(ErrorNotification),

    ShowOverview(String),
    Delete(String),

    TriggerModal,
}

pub struct Index {
    entries: Vec<ApplicationEntry>,

    fetch_task: Option<RequestHandle>,
}

impl Component for Index {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::Load);
        Self {
            entries: Vec::new(),
            fetch_task: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => match self.load(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::SetData(keys) => {
                self.entries = keys;
                self.fetch_task = None;
            }
            Msg::Error(msg) => {
                msg.toast();
            }
            Msg::ShowOverview(name) => RouteAgentDispatcher::<()>::new().send(
                RouteRequest::ChangeRoute(Route::from(AppRoute::Applications(Pages::Details {
                    name,
                    details: DetailsSection::Overview,
                }))),
            ),
            Msg::Delete(name) => match self.delete(ctx, name) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to delete", err),
            },
            Msg::TriggerModal => BackdropDispatcher::default().open(Backdrop {
                content: (html! {
                    <CreateDialog
                        backend={ctx.props().backend.clone()}
                        on_close={ctx.link().callback_once(move |_| Msg::Load)}
                        />
                }),
            }),
        };
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <>
                <PageSection variant={PageSectionVariant::Light}>
                    <Content>
                       <Flex>
                            <FlexItem>
                                <Title>{"Applications"}</Title>
                            </FlexItem>
                            <FlexItem modifiers={[FlexModifier::Align(Alignement::Right).all()]}>
                                <Button
                                        label="New Application"
                                        variant={Variant::Primary}
                                        onclick={ctx.link().callback(|_|Msg::TriggerModal)}
                                />
                            </FlexItem>
                        </Flex>
                    </Content>
                </PageSection>
                <PageSection>
                    <Table<SharedTableModel<ApplicationEntry>>
                        entries={SharedTableModel::from(self.entries.clone())}
                        header={{html_nested!{
                            <TableHeader>
                                <TableColumn label="Name"/>
                                <TableColumn label="Status"/>
                                <TableColumn label="Created"/>
                            </TableHeader>
                        }}}
                        >
                    </Table<SharedTableModel<ApplicationEntry>>>
                </PageSection>
            </>
        }
    }
}

impl Index {
    fn load(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        let link = ctx.link().clone();

        Ok(ctx.props().backend.info.request(
            Method::GET,
            "/api/registry/v1alpha1/apps",
            Nothing,
            vec![],
            ctx.callback_api::<Json<Vec<Application>>, _>(move |response| match response {
                ApiResponse::Success(entries, _) => {
                    let link = link.clone();
                    let entries = entries
                        .into_iter()
                        .map(move |app| {
                            let name = app.metadata.name.clone();
                            let name_copy = app.metadata.name.clone();

                            let on_overview = link.callback_once(move |_| Msg::ShowOverview(name));

                            ApplicationEntry {
                                app,
                                on_overview,
                                on_delete: link.callback_once(move |_| Msg::Delete(name_copy)),
                            }
                        })
                        .collect();
                    Msg::SetData(entries)
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to load")),
            }),
        )?)
    }

    fn delete(&self, ctx: &Context<Self>, name: String) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.info.request(
            Method::DELETE,
            format!("/api/registry/v1alpha1/apps/{}", url_encode(name)),
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::NO_CONTENT) => Msg::Load,
                ApiResponse::Success(_, code) => {
                    Msg::Error(format!("Unknown message code: {}", code).notify("Failed to delete"))
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to delete")),
            }),
        )?)
    }
}
