use crate::error::{ErrorNotification, ErrorNotifier};
use crate::utils::{url_encode, JsonResponse};
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

use serde_json::json;

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
    Error(ErrorNotification),

    ShowOverview(String),
    Delete(String),

    TriggerModal,
    Create,
    NewAppName(String),
}

pub struct Index {
    props: Props,
    link: ComponentLink<Self>,
    entries: Vec<ApplicationEntry>,

    new_app_name: String,

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
            new_app_name: Default::default(),
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
                msg.toast();
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
            Msg::TriggerModal => BackdropDispatcher::default().open(Backdrop {
                content: (self.create_modal()),
            }),
            Msg::Create => {
                match self.create(self.new_app_name.clone()) {
                    Ok(task) => self.fetch_task = Some(task),
                    Err(err) => error("Failed to create", err),
                }
                BackdropDispatcher::default().close();
                self.new_app_name = Default::default();
            }
            Msg::NewAppName(name) => self.new_app_name = name,
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
                        <ToolbarItem modifiers=vec![ToolbarElementModifier::Right.all()]>
                            <Button
                                    label="New Application"
                                    variant=Variant::Primary
                                    onclick=self.link.callback(|_|Msg::TriggerModal)
                            />
                        </ToolbarItem>
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
            self.link
                .callback(move |response: JsonResponse<Vec<Application>>| {
                    match response.into_body().0 {
                        Ok(entries) => {
                            let link = link.clone();
                            let entries = entries
                                .value
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
                        Err(err) => Msg::Error(err.notify("Failed to load")),
                    }
                }),
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
                    _ => Msg::Error(response.notify("Failed to delete")),
                }),
        )
    }

    fn create(&self, name: String) -> Result<FetchTask, anyhow::Error> {
        let payload = json!({
        "metadata": {
            "name": name,
        },
        "spec": {},
        });

        self.props.backend.info.request(
            Method::POST,
            "/api/registry/v1alpha1/apps",
            Json(&payload),
            vec![("Content-Type", "application/json")],
            self.link
                .callback(move |response: Response<Text>| match response.status() {
                    StatusCode::CREATED => Msg::Load,
                    _ => Msg::Error(response.notify("Failed to create")),
                }),
        )
    }

    fn create_modal(&self) -> Html {
        let v = |value: &str| match hostname_validator::is_valid(value) {
            false => InputState::Error,
            true => InputState::Default,
        };

        return html! {
            <>
            <Bullseye plain=true>
            <Modal
                title = {"Create an application"}
                variant= ModalVariant::Small
                footer = {html!{<>
                                <button class="pf-c-button pf-m-primary"
                                disabled=!hostname_validator::is_valid(self.new_app_name.as_str())
                                type="button"
                                onclick=self.link.callback(|_|Msg::Create) >
                                    {"Create"}</button>
                         </>}}
            >
                <Form>
                       <FormGroup>
                            <TextInput
                                validator=Validator::from(v)
                                onchange=self.link.callback(|app|Msg::NewAppName(app))
                                placeholder="Application ID"/>
                        </FormGroup>
                </Form>
            </Modal>
            </Bullseye>
            </>
        };
    }
}
