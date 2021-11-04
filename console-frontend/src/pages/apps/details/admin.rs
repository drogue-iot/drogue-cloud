use crate::error::error;
use crate::pages::apps::details::Props;
use crate::utils::url_encode;

use drogue_cloud_service_api::admin::{MemberEntry, Members, Role, TransferOwnership};

use anyhow::{anyhow, Result};
use core::time::Duration;
use indexmap::IndexMap;
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*, Html};

use serde_json::json;

pub struct Admin {
    props: Props,
    fetch: Option<FetchTask>,
    link: ComponentLink<Self>,

    members: Option<Users>,
    new_member_id: String,
    new_member_role: Role,

    new_owner: String,
    pending_transfer: bool,
    transfer_fetch: Option<FetchTask>,

    stop: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Users {
    app: String,
    members: Vec<User>,
    resource_version: String,
}

pub enum Msg {
    // admin
    LoadMembers,
    SetMembers(Members),
    AddMember,
    DeleteMember(String),
    NewMemberRole(Role),
    NewMemberId(String),
    NewOwner(String),
    TransferOwner,
    TransferPending(Option<TransferOwnership>),
    CancelTransfer,
    Error(String),
    Reset,
    Stop(String),
}

#[derive(Clone, Debug)]
struct User {
    id: String,
    role: Role,
    on_delete: Callback<()>,
}

impl PartialEq for User {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl TableRenderer for User {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => self.clone().id.into(),
            1 => self.clone().role.into(),
            // 3 => html! { <Button
            //              icon=Icon::ExclamationCircle
            //              variant=Variant::Link
            //              onclick=self.on_delete.clone()
            //          />
            // },
            _ => html! {},
        }
    }

    //fixme add a button at the end of the row. See render method above.
    // add fa-minus-circle in patternfly-yew
    fn actions(&self) -> Vec<DropdownChildVariant> {
        vec![html_nested! {
        <DropdownItem
            onclick=self.on_delete.clone()
        >
            {"Remove"}
        </DropdownItem>}
        .into()]
    }
}

impl Component for Admin {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        link.send_message(Msg::LoadMembers);

        Self {
            props,
            fetch: None,
            link,
            members: None,
            new_member_id: Default::default(),
            new_member_role: Role::Reader,
            new_owner: Default::default(),
            stop: false,
            pending_transfer: false,
            transfer_fetch: None,
        }
    }
    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::LoadMembers => match self.load() {
                Ok((members, transfer)) => {
                    self.fetch = Some(members);
                    self.transfer_fetch = Some(transfer);
                }
                Err(err) => error("Failed to load", err),
            },
            Msg::SetMembers(members) => {
                self.members = Some(Users::from(members, self.props.name.clone(), &self.link));
                self.fetch = None;
            }
            Msg::AddMember => {
                let (id, entry) = (&self.new_member_id, &self.new_member_role);
                if let Some(m) = self.members.as_mut() {
                    if let Err(e) = m.add(id.clone(), entry.clone(), &self.link) {
                        error("Failed to add user", e);
                    } else {
                        match self.submit() {
                            Ok(task) => self.fetch = Some(task),
                            Err(err) => error("Failed to update", err),
                        }
                    }
                }
            }
            Msg::DeleteMember(id) => {
                if let Some(m) = self.members.as_mut() {
                    m.delete(id);
                    match self.submit() {
                        Ok(task) => self.fetch = Some(task),
                        Err(err) => error("Failed to update", err),
                    }
                }
            }

            Msg::NewMemberId(id) => self.new_member_id = id,
            Msg::NewMemberRole(role) => self.new_member_role = role,

            Msg::NewOwner(id) => self.new_owner = id,
            Msg::TransferOwner => match self.transfer() {
                Ok(task) => self.fetch = Some(task),
                Err(err) => error("Failed to transfer app", err),
            },
            Msg::TransferPending(transfer) => {
                self.transfer_fetch = None;
                match transfer {
                    Some(t) => {
                        self.new_owner = t.new_user;
                        self.pending_transfer = true;
                    }
                    None => {
                        self.pending_transfer = false;
                    }
                }
            }
            Msg::CancelTransfer => match self.cancel_transfer() {
                Ok(task) => self.transfer_fetch = Some(task),
                Err(err) => error("Failed to cancel", err),
            },
            Msg::Error(msg) => {
                error("Error", msg);
                self.reset();
            }
            Msg::Reset => {
                self.reset();
            }
            Msg::Stop(msg) => {
                error("Error", msg);
                self.stop = true;
                return false;
            }
        }
        true
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.stop {
            true
        } else if self.props != props {
            self.props = props;
            true
        } else {
            false
        }
    }

    fn view(&self) -> Html {
        if let Some(m) = &self.members {
            return html! {
                <Stack gutter=true>
                    <StackItem>
                    <Card title={html!{"Application Members"}}>
                        <Table<SimpleTableModel<User>>
                                entries=SimpleTableModel::from(m.members.clone())
                                    header={html_nested!{
                                        <TableHeader>
                                            <TableColumn label="User name"/>
                                            <TableColumn label="Role"/>
                                        </TableHeader>
                                    }}
                                >
                        </Table<SimpleTableModel<User>>>
                    </Card>
                    </StackItem>
                    <StackItem>
                <Card>
                <Toolbar>
                    <ToolbarItem>
                        <TextInput
                                disabled=self.fetch.is_some()
                                onchange=self.link.callback(|id|Msg::NewMemberId(id))
                                placeholder="User id"/>
                        </ToolbarItem>
                        <ToolbarItem>
                            <Select<Role> placeholder="Select user role" variant=SelectVariant::Single(self.link.callback(Msg::NewMemberRole))>
                                <SelectOption<Role> value=Role::Reader description="Read-only access" />
                                <SelectOption<Role> value=Role::Manager description="Read-write access" />
                                <SelectOption<Role> value=Role::Admin description="Administrative access" />
                            </Select<Role>>
                        </ToolbarItem>
                        <ToolbarItem>
                            <Button
                                label="Add"
                                icon=Icon::PlusCircleIcon
                                onclick=self.link.callback(|_|Msg::AddMember)
                            />
                    </ToolbarItem>
                </Toolbar>
                 </Card>
                </StackItem>
                <StackItem>
                <Card title={html!{"Transfer application ownership"}}>
                   <Toolbar>
                        <ToolbarGroup>
                            <ToolbarItem>
                                <TextInput
                                    onchange=self.link.callback(|user|Msg::NewOwner(user))
                                    placeholder="Username"/>
                            </ToolbarItem>
                            <ToolbarItem>
                                <Button
                                        disabled=self.transfer_fetch.is_some()
                                        label="Transfer"
                                        variant=Variant::Primary
                                        onclick=self.link.callback(|_|Msg::TransferOwner)
                                />
                            </ToolbarItem>
                            <ToolbarItem>
                                <Button
                                        disabled=!self.pending_transfer
                                        label="Cancel"
                                        variant=Variant::Secondary
                                        onclick=self.link.callback(|_|Msg::CancelTransfer)
                                />
                            </ToolbarItem>
                        </ToolbarGroup>
                </Toolbar>
                // todo
                // if pending_transfer {
                        // There is a currently a pending transfer to : <user>
                //     }
                </Card>
                </StackItem>
                </Stack>
            };
        } else {
            return html! {};
        }
    }
}

impl Admin {
    fn load(&self) -> Result<(FetchTask, FetchTask), anyhow::Error> {
        let members = self.load_members();
        let transfer = self.load_transfer_state();

        match (members, transfer) {
            (Ok(members), Ok(transfer)) => Ok((members, transfer)),
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
        }
    }

    fn load_members(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::GET,
            format!(
                "/api/admin/v1alpha1/apps/{}/members",
                url_encode(&self.props.name)
            ),
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<Members, anyhow::Error>>>| match response
                    .status()
                {
                    StatusCode::OK => match response.into_body().0 {
                        Ok(content) => Msg::SetMembers(content),
                        Err(err) => Msg::Error(err.to_string()),
                    },
                    StatusCode::NOT_FOUND => {
                        Msg::Stop("You are not an administrator for this app".to_string())
                    }
                    status => Msg::Error(format!("Failed to fetch members. {}", status)),
                },
            ),
        )
    }

    fn submit(&self) -> Result<FetchTask, anyhow::Error> {
        if let Some(m) = &self.members {
            let members = m.serialize();
            self.props.backend.info.request(
                Method::PUT,
                format!(
                    "/api/admin/v1alpha1/apps/{}/members",
                    url_encode(&self.props.name)
                ),
                Json(&members),
                vec![("Content-Type", "application/json")],
                self.link
                    .callback(move |response: Response<Text>| match response.status() {
                        StatusCode::NO_CONTENT => {
                            ToastDispatcher::default().toast(Toast {
                                title: "Success !".into(),
                                body: html! {<>
                                    <Content>
                                    <p>{"Application members saved."}</p>
                                    </Content>
                                </>},
                                r#type: Type::Success,
                                timeout: Some(Duration::from_secs(3)),
                                ..Default::default()
                            });
                            Msg::LoadMembers
                        }
                        status => Msg::Error(format!(
                            "Failed to perform update: Code {}. {}",
                            status,
                            response
                                .body()
                                .as_ref()
                                .unwrap_or(&"Unknown error.".to_string())
                        )),
                    }),
            )
        } else {
            Err(anyhow!("Nothing to save"))
        }
    }

    fn reset(&mut self) {
        self.fetch = None;
        self.link.send_message(Msg::LoadMembers);
    }

    fn transfer(&self) -> Result<FetchTask, anyhow::Error> {
        let payload = json!(TransferOwnership {
            new_user: self.new_owner.clone()
        });
        let link = self
            .props
            .endpoints
            .console
            .clone()
            .map(|console| format!("{}/transfer/{}", console, url_encode(&self.props.name)))
            .unwrap_or_else(|| "Error while creating the link".into());

        self.props.backend.info.request(
            Method::PUT,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&self.props.name)
            ),
            Json(&payload),
            vec![("Content-Type", "application/json")],
            self.link
                .callback(move |response: Response<Text>| match response.status() {
                    StatusCode::ACCEPTED => {
                        ToastDispatcher::default().toast(Toast {
                            title: "Success !".into(),
                            body: html! {<>
                                <Content>
                                <p>{"Ownership transfer initiated. Share this link with the user:"}</p>
                                <p>
                                    <Clipboard value=link.clone()
                                        readonly=true
                                    />
                                </p>
                                </Content>
                            </>},
                            r#type: Type::Success,
                            ..Default::default()
                        });
                        Msg::Reset
                    }
                    status => Msg::Error(format!(
                        "Failed to submit: Code {}. {}",
                        status,
                        response
                            .body()
                            .as_ref()
                            .unwrap_or(&"Unknown error.".to_string())
                    )),
                }),
        )
    }

    fn cancel_transfer(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::DELETE,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&self.props.name)
            ),
            Nothing,
            vec![],
            self.link
                .callback(move |response: Response<Text>| match response.status() {
                    StatusCode::NO_CONTENT => Msg::TransferPending(None),
                    status => Msg::Error(format!("Failed to cancel transfer. {}", status)),
                }),
        )
    }

    fn load_transfer_state(&self) -> Result<FetchTask, anyhow::Error> {
        self.props.backend.info.request(
            Method::GET,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&self.props.name)
            ),
            Nothing,
            vec![],
            self.link.callback(
                move |response: Response<Json<Result<TransferOwnership, anyhow::Error>>>| {
                    match response.status() {
                        StatusCode::OK => match response.into_body().0 {
                            Ok(user) => Msg::TransferPending(Some(user)),
                            Err(err) => Msg::Error(err.to_string()),
                        },
                        StatusCode::NO_CONTENT => Msg::TransferPending(None),
                        status => Msg::Error(format!("Failed to fetch transfer state. {}", status)),
                    }
                },
            ),
        )
    }
}

impl Users {
    pub fn from(members: Members, app: String, link: &ComponentLink<Admin>) -> Self {
        let mut new_members: Vec<User> = Vec::new();

        for (user, role) in members.members {
            new_members.push(User {
                id: user.clone(),
                role: role.role,
                on_delete: link.callback(move |_| Msg::DeleteMember(user.clone())),
            });
        }

        Users {
            app,
            members: new_members,
            resource_version: members.resource_version.unwrap_or_default(),
        }
    }

    //FIXME : remove the dependency on IndexMap by using generics for the Member struct :)
    pub fn serialize(&self) -> Members {
        let mut members: IndexMap<String, MemberEntry> = IndexMap::new();

        for u in &self.members {
            members.insert(u.id.clone(), MemberEntry { role: u.role });
        }

        Members {
            members,
            resource_version: Some(self.resource_version.clone()),
        }
    }

    pub fn delete(&mut self, id: String) {
        self.members.retain(|u| *u.id != id);
    }

    pub fn add(&mut self, id: String, role: Role, link: &ComponentLink<Admin>) -> Result<()> {
        let copy_id = id.clone();
        let user = User {
            id: id.clone(),
            role,
            on_delete: link.callback(move |_| Msg::DeleteMember(copy_id.clone())),
        };

        if self.contains(id.clone()) {
            Err(anyhow!("User is already a member"))
        } else {
            self.members.push(user);
            Ok(())
        }
    }

    pub fn contains(&self, id: String) -> bool {
        let user = &User {
            id: id.clone(),
            // does not matter for the equal operation. See PartialEq impl above.
            role: Role::Reader,
            on_delete: Default::default(),
        };
        return self.members.contains(user);
    }
}
