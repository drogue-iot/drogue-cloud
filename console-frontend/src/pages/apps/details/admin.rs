use crate::backend::{ApiError, ApiResponse, Json, JsonHandlerScopeExt, Nothing, RequestHandle};
use crate::html_prop;
use crate::utils::{success, ToastBuilder};
use crate::{
    error::{error, ErrorNotification, ErrorNotifier},
    pages::apps::details::Props,
    utils::url_encode,
};
use anyhow::{anyhow, Result};
use drogue_cloud_service_api::admin::{MemberEntry, Members, Role, Roles, TransferOwnership};
use http::{Method, StatusCode};
use indexmap::IndexMap;
use patternfly_yew::*;
use yew::html::Scope;
use yew::{prelude::*, Html};

pub struct Admin {
    fetch: Option<RequestHandle>,

    members: Option<Users>,
    new_member_id: String,
    new_member_roles: Vec<Role>,

    new_owner: String,
    pending_transfer: bool,
    transfer_fetch: Option<RequestHandle>,
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
    NewMemberRoles(Vec<Role>),
    NewMemberId(String),
    NewOwner(String),
    TransferOwner,
    TransferPending(Option<TransferOwnership>),
    CancelTransfer,
    Error(ErrorNotification),
    Reset,
    Stop(Option<ErrorNotification>),
}

#[derive(Clone, Debug)]
struct User {
    id: String,
    roles: Vec<Role>,
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
            1 => self
                .roles
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<String>>()
                .join(", ")
                .into(),
            _ => return html! {},
        }
    }

    //fixme add a button at the end of the row. See render method above.
    // add fa-minus-circle in patternfly-yew
    fn actions(&self) -> Vec<DropdownChildVariant> {
        vec![html_nested! {
        <DropdownItem
            onclick={self.on_delete.clone()}
        >
            {"Remove"}
        </DropdownItem>}
        .into()]
    }
}

impl Component for Admin {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_message(Msg::LoadMembers);

        Self {
            fetch: None,
            members: None,
            new_member_id: Default::default(),
            new_member_roles: Vec::new(),
            new_owner: Default::default(),
            stop: false,
            pending_transfer: false,
            transfer_fetch: None,
            can_transfer: false,
        }
    }
    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadMembers => match self.load(ctx) {
                Ok((members, transfer)) => {
                    self.fetch = Some(members);
                    self.transfer_fetch = Some(transfer);
                }
                Err(err) => error("Failed to load", err),
            },
            Msg::SetMembers(members) => {
                self.members = Some(Users::from(members, ctx.props().name.clone(), ctx.link()));
                self.fetch = None;
            }
            Msg::AddMember => {
                let (id, roles) = (&self.new_member_id, &self.new_member_roles);
                if let Some(m) = self.members.as_mut() {
                    if let Err(e) = m.add(id.clone(), roles.clone(), ctx.link()) {
                        error("Failed to add user", e);
                    } else {
                        match self.submit(ctx) {
                            Ok(task) => self.fetch = Some(task),
                            Err(err) => error("Failed to update", err),
                        }
                    }
                }
            }
            Msg::DeleteMember(id) => {
                if let Some(m) = self.members.as_mut() {
                    m.delete(id);
                    match self.submit(ctx) {
                        Ok(task) => self.fetch = Some(task),
                        Err(err) => error("Failed to update", err),
                    }
                }
            }

            Msg::NewMemberId(id) => self.new_member_id = id,
            Msg::NewMemberRoles(roles) => self.new_member_roles = roles,

            Msg::NewOwner(id) => self.new_owner = id,
            Msg::TransferOwner => match self.transfer(ctx) {
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
            Msg::CancelTransfer => match self.cancel_transfer(ctx) {
                Ok(task) => self.transfer_fetch = Some(task),
                Err(err) => error("Failed to cancel", err),
            },
            Msg::Error(err) => {
                err.toast();
                self.reset(ctx);
            }
            Msg::Reset => {
                self.reset(ctx);
            }
            Msg::Stop(err) => {
                self.stop = true;
                if let Some(err) = err {
                    err.toast();
                }
            }
        }
        true
    }

    fn changed(&mut self, _: &Context<Self>, _: &<Self as yew::Component>::Properties) -> bool {
        !self.stop
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        if let Some(m) = &self.members {
            html! {
                <Stack gutter=true>
                    <StackItem>
                        <Card title={html_prop!({"Application Members"})}>
                            <Table<SharedTableModel<User>>
                                mode={TableMode::Compact}
                                entries={SharedTableModel::from(m.members.clone())}
                                    header={{html_nested!{
                                        <TableHeader>
                                            <TableColumn label="User name"/>
                                            <TableColumn label="Roles"/>
                                        </TableHeader>
                                    }}}
                            />
                        </Card>
                    </StackItem>
                    <StackItem>
                        <Card>
                            <Toolbar>
                                <ToolbarItem>
                                    <TextInput
                                            disabled={self.fetch.is_some()}
                                            onchange={ctx.link().callback(Msg::NewMemberId)}
                                            placeholder="User name"/>
                                    </ToolbarItem>
                                    <ToolbarItem>
                                        <Select<Role>
                                                placeholder="Select user roles"
                                                multiple=true
                                                variant={SelectVariant::Checkbox(ctx.link().callback(Msg::NewMemberRoles))}
                                                chip={ChipVariant::Values}>
                                            <SelectOption<Role> value={Role::Reader} description="Read-only for app and devices details" />
                                            <SelectOption<Role> value={Role::Manager} description="Read-write for app and devices details" />
                                            <SelectOption<Role> value={Role::Subscriber} description="Consume app events" />
                                            <SelectOption<Role> value={Role::Publisher} description="Publish commands" />
                                            <SelectOption<Role> value={Role::Admin} description="All access" />
                                        </Select<Role>>
                                    </ToolbarItem>
                                    <ToolbarItem>
                                        <Button
                                            label="Add"
                                            icon={Icon::PlusCircleIcon}
                                            onclick={ctx.link().callback(|_|Msg::AddMember)}
                                        />
                                </ToolbarItem>
                            </Toolbar>
                        </Card>
                    </StackItem>

                    if self.can_transfer {
                    <StackItem>
                        <Card title={html_prop!({"Transfer application ownership"})}>
                            if self.pending_transfer {
                                <p>{format!("Pending transfer to user: {}", self.new_owner)}</p>
                            }
                           <Toolbar>
                                <ToolbarGroup>
                                    <ToolbarItem>
                                        <TextInput
                                            onchange={ctx.link().callback(Msg::NewOwner)}
                                            placeholder="Username"/>
                                    </ToolbarItem>
                                    <ToolbarItem>
                                        <Button
                                                disabled={self.transfer_fetch.is_some()}
                                                label="Transfer"
                                                variant={Variant::Primary}
                                                onclick={ctx.link().callback(|_|Msg::TransferOwner)}
                                        />
                                    </ToolbarItem>
                                    <ToolbarItem>
                                        <Button
                                                disabled={!self.pending_transfer}
                                                label="Cancel"
                                                variant={Variant::Secondary}
                                                onclick={ctx.link().callback(|_|Msg::CancelTransfer)}
                                        />
                                    </ToolbarItem>
                                </ToolbarGroup>
                        </Toolbar>
                        </Card>
                    </StackItem>
                    }

                </Stack>
            }
        } else {
            html! {}
        }
    }
}

impl Admin {
    fn load(&self, ctx: &Context<Self>) -> Result<(RequestHandle, RequestHandle), anyhow::Error> {
        let members = self.load_members(ctx);
        let transfer = self.load_transfer_state(ctx);

        match (members, transfer) {
            (Ok(members), Ok(transfer)) => Ok((members, transfer)),
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
        }
    }

    fn load_members(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::GET,
            format!(
                "/api/admin/v1alpha1/apps/{}/members",
                url_encode(&ctx.props().name)
            ),
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<Json<Members>, _>(move |response| match response {
                ApiResponse::Success(members, _) => Msg::SetMembers(members),
                ApiResponse::Failure(ApiError::Response(_, StatusCode::NOT_FOUND)) => {
                    Msg::Stop(Some(
                        "You are not an administrator for this app"
                            .notify("Failed to fetch members"),
                    ))
                }
                ApiResponse::Failure(err) => Msg::Stop(Some(err.notify("Failed to fetch members"))),
            }),
        )?)
    }

    fn submit(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        if let Some(m) = &self.members {
            let members = m.serialize();
            Ok(ctx.props().backend.request(
                Method::PUT,
                format!(
                    "/api/admin/v1alpha1/apps/{}/members",
                    url_encode(&ctx.props().name)
                ),
                vec![],
                Json(members),
                vec![],
                ctx.callback_api::<(), _>(move |response| match response {
                    ApiResponse::Success(_, StatusCode::NO_CONTENT) => {
                        success("Application members saved.");
                        Msg::LoadMembers
                    }
                    ApiResponse::Success(_, code) => Msg::Error(
                        format!("Unknown response code: {}", code).notify("Update failed"),
                    ),
                    ApiResponse::Failure(err) => Msg::Error(err.notify("Update failed")),
                }),
            )?)
        } else {
            Err(anyhow!("Nothing to save"))
        }
    }

    fn reset(&mut self, ctx: &Context<Self>) {
        self.fetch = None;
        ctx.link().send_message(Msg::LoadMembers);
    }

    fn transfer(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        let payload = TransferOwnership {
            new_user: self.new_owner.clone(),
        };
        let link = ctx
            .props()
            .endpoints
            .console
            .clone()
            .map(|console| format!("{}/transfer/{}", console, url_encode(&ctx.props().name)))
            .unwrap_or_else(|| "Error while creating the link".into());

        Ok(ctx.props().backend.request(
            Method::PUT,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&ctx.props().name)
            ),
            vec![],
            Json(payload),
            vec![],
            ctx.callback_api::<(), _>(move |response| match response {
                ApiResponse::Success(_, StatusCode::ACCEPTED) => {
                    let body = html! {
                        <Content>
                            <p>{"Ownership transfer initiated. Share this link with the user:"}</p>
                            <p>
                                <Clipboard value={link.clone()}
                                    readonly=true
                                />
                            </p>
                        </Content>
                    };
                    ToastBuilder::success().title("Success").body(body).toast();
                    Msg::Reset
                }
                ApiResponse::Success(_, code) => {
                    Msg::Error(format!("Invalid response code: {}", code).notify("Transfer failed"))
                }
                ApiResponse::Failure(err) => Msg::Error(err.notify("Transfer failed")),
            }),
        )?)
    }

    fn cancel_transfer(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::DELETE,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&ctx.props().name)
            ),
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<(), _>(|response| match response {
                ApiResponse::Success(..) => Msg::TransferPending(None),
                ApiResponse::Failure(err) => Msg::Error(err.notify("Unable to cancel transfer")),
            }),
        )?)
    }

    fn load_transfer_state(&self, ctx: &Context<Self>) -> Result<RequestHandle, anyhow::Error> {
        Ok(ctx.props().backend.request(
            Method::GET,
            format!(
                "/api/admin/v1alpha1/apps/{}/transfer-ownership",
                url_encode(&ctx.props().name)
            ),
            vec![],
            Nothing,
            vec![],
            ctx.callback_api::<Option<Json<TransferOwnership>>, _>(move |response| {
                log::info!("Response: {:?}", response);
                match response {
                    ApiResponse::Success(transfer, StatusCode::OK | StatusCode::NO_CONTENT) => {
                        Msg::TransferPending(transfer)
                    }
                    ApiResponse::Success(..) => Msg::Stop(Some(
                        "Unknown response".notify("Failed to load transfer state"),
                    )),
                    ApiResponse::Failure(ApiError::Response(_, StatusCode::NOT_FOUND)) => {
                        Msg::Stop(None)
                    }
                    ApiResponse::Failure(err) => {
                        Msg::Stop(Some(err.notify("Failed loading transfer state")))
                    }
                }
            }),
        )?)
    }
}

impl Users {
    pub fn from(members: Members, app: String, link: &Scope<Admin>) -> Self {
        let mut new_members: Vec<User> = Vec::new();

        for (user, roles) in members.members {
            new_members.push(User {
                id: user.clone(),
                roles: roles.roles.0,
                on_delete: link.callback(move |_| Msg::DeleteMember(user.clone())),
            });
        }

        Users {
            app,
            members: new_members,
            resource_version: members.resource_version.unwrap_or_default(),
        }
    }

    pub fn serialize(&self) -> Members {
        let mut members: IndexMap<String, MemberEntry> = IndexMap::new();

        for u in &self.members {
            members.insert(
                u.id.clone(),
                MemberEntry {
                    roles: Roles(u.roles.clone()),
                },
            );
        }

        Members {
            members,
            resource_version: Some(self.resource_version.clone()),
        }
    }

    pub fn delete(&mut self, id: String) {
        self.members.retain(|u| *u.id != id);
    }

    pub fn add(&mut self, id: String, roles: Vec<Role>, link: &Scope<Admin>) -> Result<()> {
        let copy_id = id.clone();
        let user = User {
            id: id.clone(),
            roles,
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
            roles: vec![],
            on_delete: Default::default(),
        };
        return self.members.contains(user);
    }
}
