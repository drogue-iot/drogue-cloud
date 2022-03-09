use crate::backend::{ApiResponse, Json, JsonHandlerScopeExt, Nothing, RequestHandle};
use crate::error::{ErrorNotification, ErrorNotifier};
use crate::{
    backend::Backend,
    data::{SharedDataDispatcher, SharedDataOps},
    error::error,
    page::AppRoute,
    pages::{
        apps::ApplicationContext,
        devices::{CreateDialog, DetailsSection, Pages},
        HasReadyState,
    },
    utils::{navigate_to, url_encode},
};
use drogue_client::registry::v1::{Application, Device};
use http::{Method, StatusCode};
use patternfly_yew::*;
use yew::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceEntry {
    pub device: Device,
    pub on_overview: Callback<()>,
    pub on_delete: Callback<()>,
}

impl TableRenderer for DeviceEntry {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => html! {
                <a onclick={self.on_overview.clone().reform(|_|())}>{self.device.metadata.name.clone()}</a>
            },
            1 => self.device.render_state(),
            2 => self
                .device
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
    pub app: String,
}

pub enum Msg {
    LoadApps,
    Load,
    SetData(Vec<DeviceEntry>),
    SetApps(Vec<String>),
    SetApp(String),
    Error(ErrorNotification),

    AppSearch(String),

    ShowOverview(String),
    Delete(String),
    TriggerModal,
}

pub struct Index {
    fetch_task: Option<RequestHandle>,

    entries: Vec<DeviceEntry>,
    app: String,
    app_filter: String,
    apps: Vec<String>,
}

impl Component for Index {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::LoadApps);

        let app = ctx.props().app.clone();

        Self {
            entries: Vec::new(),
            fetch_task: None,
            app,
            app_filter: String::new(),
            apps: Vec::new(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadApps => match self.load_apps(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::Load => match self.load(ctx) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::SetApps(apps) => {
                self.fetch_task = None;

                if self.app.is_empty() {
                    // if we don't have an app set yet, set the first one
                    if let Some(app) = apps.first() {
                        ctx.link().send_message(Msg::SetApp(app.clone()));
                    }
                } else {
                    ctx.link().send_message(Msg::Load);
                }

                self.apps = apps;
            }
            Msg::SetData(keys) => {
                self.entries = keys;
                self.fetch_task = None;
            }
            Msg::SetApp(app) => {
                if self.app != app {
                    let ctx = ApplicationContext::Single(app);
                    SharedDataDispatcher::new().set(ctx.clone());
                    navigate_to(AppRoute::Devices(Pages::Index { app: ctx }));
                }
            }
            Msg::Error(msg) => {
                msg.toast();
            }
            Msg::ShowOverview(name) => navigate_to(AppRoute::Devices(Pages::Details {
                app: ApplicationContext::Single(self.app.clone()),
                name,
                details: DetailsSection::Overview,
            })),
            Msg::AppSearch(value) => {
                self.app_filter = value;
            }
            Msg::TriggerModal => BackdropDispatcher::default().open(Backdrop {
                content: (html! {
                    <CreateDialog
                        backend={ctx.props().backend.clone()}
                        on_close={ctx.link().callback_once(move |_| Msg::Load)}
                        app={self.app.clone()}
                        />
                }),
            }),
            Msg::Delete(name) => match self.delete(ctx, name) {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to delete", err),
            },
        };
        true
    }

    fn changed(&mut self, ctx: &Context<Self>) -> bool {
        if self.app != ctx.props().app {
            self.app = ctx.props().app.clone();
            ctx.link().send_message(Msg::Load);
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link().clone();
        let app_filter = self.app_filter.clone();
        return html! {
            <>
                <PageSection variant={PageSectionVariant::Light}>
                    <ContextSelector
                        selected={self.app.clone()}
                        onsearch={link.callback(Msg::AppSearch)}
                        >
                        { for self.apps.iter().filter(|app|{
                            if app_filter.is_empty() {
                                true
                            } else {
                                app.contains(&app_filter)
                            }
                        }).map(|app|{
                            let ac = app.clone();
                            let onclick = link.callback(move |_|Msg::SetApp(ac.clone()));
                            html_nested!{
                                <ContextSelectorItem
                                    label={app.clone()}
                                    onclick={onclick}
                                />}
                        })}
                    </ContextSelector>
                </PageSection>
                <PageSection variant={PageSectionVariant::Light}>
                    <Content>
                        <Flex>
                        <FlexItem>
                            <Title>{"Devices"}</Title>
                        </FlexItem>
                        <FlexItem modifiers={[FlexModifier::Align(Alignement::Right).all()]}>
                            <Button
                                    label="New device"
                                    disabled={self.app.is_empty()}
                                    variant={Variant::Primary}
                                    onclick={ctx.link().callback(|_|Msg::TriggerModal)}
                            />
                        </FlexItem>
                        </Flex>
                    </Content>
                </PageSection>
            { if self.app.is_empty() {html!{
            }} else { html!{
                <PageSection>
                    <Table<SharedTableModel<DeviceEntry>>
                        entries={SharedTableModel::from(self.entries.clone())}
                        header={{html_nested!{
                            <TableHeader>
                                <TableColumn label="Name"/>
                                <TableColumn label="Status"/>
                                <TableColumn label="Created"/>
                            </TableHeader>
                        }}}
                        >
                    </Table<SharedTableModel<DeviceEntry>>>
                </PageSection>
            }}}
            </>
        };
    }
}

impl Index {
    fn load(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        let link = ctx.link().clone();

        Ok(ctx.props().backend.info.request(
            Method::GET,
            format!(
                "/api/registry/v1alpha1/apps/{}/devices",
                url_encode(&self.app)
            ),
            Nothing,
            vec![],
            ctx.callback_api::<Json<Vec<Device>>, _>(move |response| match response {
                ApiResponse::Success(entries, _) => {
                    let entries = entries
                        .into_iter()
                        .map(move |device| {
                            let name = device.metadata.name.clone();
                            let name_copy = device.metadata.name.clone();
                            let on_overview = link.callback_once(move |_| Msg::ShowOverview(name));
                            let on_delete = link.callback_once(move |_| Msg::Delete(name_copy));

                            DeviceEntry {
                                device,
                                on_overview,
                                on_delete,
                            }
                        })
                        .collect();
                    Msg::SetData(entries)
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to load device")),
            }),
        )?)
    }

    fn load_apps(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.info.request(
            Method::GET,
            "/api/registry/v1alpha1/apps",
            Nothing,
            vec![],
            ctx.callback_api::<Json<Vec<Application>>, _>(move |response| match response {
                ApiResponse::Success(entries, _) => {
                    let entries = entries
                        .into_iter()
                        .map(move |app| app.metadata.name)
                        .collect();
                    Msg::SetApps(entries)
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Failed to load applications")),
            }),
        )?)
    }

    fn delete(&self, ctx: &Context<Self>, name: String) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.info.request(
            Method::DELETE,
            format!(
                "/api/registry/v1alpha1/apps/{}/devices/{}",
                url_encode(&self.app),
                url_encode(name)
            ),
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
