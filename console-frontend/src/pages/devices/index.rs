use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::JsonResponse;
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
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*};

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
                <a onclick=self.on_overview.clone().reform(|_|())>{self.device.metadata.name.clone()}</a>
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
    TriggerModal,
}

pub struct Index {
    props: Props,
    link: ComponentLink<Self>,

    fetch_task: Option<FetchTask>,

    entries: Vec<DeviceEntry>,
    app: String,
    app_filter: String,
    apps: Vec<String>,
}

impl Component for Index {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::LoadApps);

        let app = props.app.clone();

        Self {
            props,
            link,
            entries: Vec::new(),
            fetch_task: None,
            app,
            app_filter: String::new(),
            apps: Vec::new(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::LoadApps => match self.load_apps() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::Load => match self.load() {
                Ok(task) => self.fetch_task = Some(task),
                Err(err) => error("Failed to fetch", err),
            },
            Msg::SetApps(apps) => {
                self.fetch_task = None;

                if self.app.is_empty() {
                    // if we don't have an app set yet, set the first one
                    if let Some(app) = apps.first() {
                        self.link.send_message(Msg::SetApp(app.clone()));
                    }
                } else {
                    self.link.send_message(Msg::Load);
                }

                self.apps = apps;
            }
            Msg::SetData(keys) => {
                self.entries = keys;
                self.fetch_task = None;
            }
            Msg::SetApp(app) => {
                if self.app != app {
                    let ctx = ApplicationContext::Single(app.clone());
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
                        backend=self.props.backend.clone()
                        on_close=self.link.callback_once(move |_| Msg::Load)
                        app=self.app.clone()
                        />
                }),
            }),
        };
        true
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        let changed = if self.props != props {
            self.props = props;
            true
        } else {
            false
        };

        if changed && self.app != self.props.app {
            self.app = self.props.app.clone();
            self.link.send_message(Msg::Load);
        }

        changed
    }

    fn view(&self) -> Html {
        let link = self.link.clone();
        let app_filter = self.app_filter.clone();
        return html! {
            <>
                <PageSection variant=PageSectionVariant::Light>
                    <ContextSelector
                        selected=self.app.clone()
                        onsearch=link.callback(|v|Msg::AppSearch(v))
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
                                    label=app.clone()
                                    onclick=onclick
                                />}
                        })}
                    </ContextSelector>
                </PageSection>
                <PageSection variant=PageSectionVariant::Light>
                    <Content>
                        <Flex>
                        <FlexItem>
                            <Title>{"Devices"}</Title>
                        </FlexItem>
                        <FlexItem modifiers=vec![FlexModifier::Align(Alignement::Right).all()]>
                            <Button
                                    label="New device"
                                    variant=Variant::Primary
                                    onclick=self.link.callback(|_|Msg::TriggerModal)
                            />
                        </FlexItem>
                        </Flex>
                    </Content>
                </PageSection>
            { if self.app.is_empty() {html!{
            }} else { html!{
                <PageSection>
                    <Table<SimpleTableModel<DeviceEntry>>
                        entries=SimpleTableModel::from(self.entries.clone())
                        header={html_nested!{
                            <TableHeader>
                                <TableColumn label="Name"/>
                                <TableColumn label="Status"/>
                                <TableColumn label="Created"/>
                            </TableHeader>
                        }}
                        >
                    </Table<SimpleTableModel<DeviceEntry>>>
                </PageSection>
            }}}
            </>
        };
    }
}

impl Index {
    fn load(&self) -> Result<FetchTask, anyhow::Error> {
        let link = self.link.clone();

        self.props.backend.info.request(
            Method::GET,
            format!(
                "/api/registry/v1alpha1/apps/{}/devices",
                url_encode(&self.app)
            ),
            Nothing,
            vec![],
            self.link
                .callback(move |response: JsonResponse<Vec<Device>>| {
                    match response.into_body().0 {
                        Ok(entries) => {
                            let link = link.clone();
                            let entries = entries
                                .value
                                .into_iter()
                                .map(move |device| {
                                    let name = device.metadata.name.clone();
                                    let on_overview =
                                        link.callback_once(move |_| Msg::ShowOverview(name));

                                    DeviceEntry {
                                        device,
                                        on_overview,
                                        on_delete: Default::default(),
                                    }
                                })
                                .collect();
                            Msg::SetData(entries)
                        }
                        Err(err) => Msg::Error(err.notify("Failed to load device")),
                    }
                }),
        )
    }

    fn load_apps(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::GET,
            "/api/registry/v1alpha1/apps",
            Nothing,
            vec![],
            self.link
                .callback(move |response: JsonResponse<Vec<Application>>| {
                    match response.into_body().0 {
                        Ok(entries) => {
                            let entries = entries
                                .value
                                .into_iter()
                                .map(move |app| app.metadata.name)
                                .collect();
                            Msg::SetApps(entries)
                        }
                        Err(err) => Msg::Error(err.notify("Failed to load applications")),
                    }
                }),
        )
    }
}
