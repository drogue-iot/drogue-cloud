use crate::utils::success;
use crate::{
    error::{error, ErrorNotification, ErrorNotifier},
    pages::apps::details::Props,
    utils::{url_encode, Json, JsonResponse},
};
use anyhow::{anyhow, Result};
use drogue_cloud_service_api::admin::{MemberEntry, Members, Role, TransferOwnership};
use indexmap::IndexMap;
use patternfly_yew::*;
use serde_json::json;
use yew::{format::*, prelude::*, services::fetch::*, Html};

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
    can_transfer: bool,

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
    Error(ErrorNotification),
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
            0 => {
                if self.id.is_empty() {
                    return html! {<i>{"anonymous"}</i>};
                } else {
                    self.id.clone().into()
                }
            }
            1 => self.role.into(),
            _ => return html! {},
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
            can_transfer: false,
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
                    if let Err(e) = m.add(id.clone(), *entry, &self.link) {
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
                self.can_transfer = true;
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
            Msg::Error(err) => {
                err.toast();
                self.reset();
            }
            Msg::Reset => {
                self.reset();
            }
            Msg::Stop(msg) => {
                if !msg.is_empty() {
                    error("Error", msg);
                }
                self.stop = true;
                return false;
            }
        }
        true
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.stop {
            false
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
                                mode=TableMode::Compact
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
                    { if self.can_transfer {
                        html!{
                            <StackItem>
                                <Card title={html!{"Transfer application ownership"}}>
                                { if self.pending_transfer {
                                        html!{<p>{format!("Pending transfer to user: {}", self.new_owner)}</p>}
                                } else {html!{}}}
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
                                </Card>
                            </StackItem>
                        }
                    } else {
                    html!{}
                    }}
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
                move |response: JsonResponse<Members>| match response.status() {
                    StatusCode::OK => match response.into_body().0 {
                        Ok(content) => Msg::SetMembers(content.value),
                        Err(err) => Msg::Error(err.notify("Failed to fetch members")),
                    },
                    StatusCode::NOT_FOUND => {
                        Msg::Stop("You are not an administrator for this app".to_string())
                    }
                    _ => Msg::Error(response.notify("Failed to fetch members")),
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
                            success("Application members saved.");
                            Msg::LoadMembers
                        }
                        _ => Msg::Error(response.notify("Update failed")),
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
                        success(html! {
                            <Content>
                                <p>{"Ownership transfer initiated. Share this link with the user:"}</p>
                                <p>
                                    <Clipboard value=link.clone()
                                        readonly=true
                                    />
                                </p>
                            </Content>
                        });
                        Msg::Reset
                    }
                    _ => Msg::Error(response.notify("Transfer failed")),
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
                    _ => Msg::Error(response.notify("Unable to cancel transfer")),
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
            self.link
                .callback(move |response: JsonResponse<TransferOwnership>| {
                    match response.status() {
                        StatusCode::OK => match response.into_body().0 {
                            Ok(user) => Msg::TransferPending(Some(user.value)),
                            Err(err) => Msg::Error(err.notify("Failed to fetch transfer state")),
                        },
                        StatusCode::NO_CONTENT => Msg::TransferPending(None),
                        StatusCode::NOT_FOUND => Msg::Stop("".to_string()),
                        status => Msg::Error(
                            response
                                .notify(format!("Error while fetching transfer state: {}", status)),
                        ),
                    }
                }),
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

        if self.contains(id) {
            Err(anyhow!("User is already a member"))
        } else {
            self.members.push(user);
            Ok(())
        }
    }

    pub fn contains(&self, id: String) -> bool {
        let user = &User {
            id,
            // does not matter for the equal operation. See PartialEq impl above.
            role: Role::Reader,
            on_delete: Default::default(),
        };
        return self.members.contains(user);
    }
}
