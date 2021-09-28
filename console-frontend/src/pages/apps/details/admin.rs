use crate::error::error;
use crate::pages::apps::details::Props;
use crate::utils::url_encode;

use drogue_cloud_service_api::admin::{MemberEntry, Members, Role};

use anyhow::{anyhow, Result};
use core::time::Duration;
use indexmap::IndexMap;
use patternfly_yew::*;
use yew::{format::*, prelude::*, services::fetch::*, Html};

pub struct Admin {
    props: Props,
    fetch: Option<FetchTask>,
    link: ComponentLink<Self>,

    members: Option<Users>,
    new_member_id: String,
    new_member_role: Role,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Users {
    app: String,
    managers: Vec<User>,
    readers: Vec<User>,
    admin: String,
    resource_version: String,
}

#[derive(Debug)]
pub enum Msg {
    // admin
    LoadMembers,
    SetMembers(Members),
    AddMember,
    DeleteMember(String),
    SaveMembers,
    NewMemberRole(Role),
    NewMemberId(String),
    Error(String),
}

#[derive(Clone, Debug)]
struct User {
    id: String,
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
            1 => self.clone().id.into(),
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
        }
    }
    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        log::warn!("UPDATE msg is : {:?}", msg);
        match msg {
            Msg::LoadMembers => match self.load() {
                Ok(task) => self.fetch = Some(task),
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
                    }
                }
            }
            Msg::DeleteMember(id) => {
                if let Some(m) = self.members.as_mut() {
                    m.delete(id);
                }
            }
            Msg::SaveMembers => {
                if let Some(members) = &self.members {
                    match self.submit(&members.serialize()) {
                        Ok(task) => self.fetch = Some(task),
                        Err(err) => error("Failed to update", err),
                    }
                }
            }

            Msg::NewMemberId(id) => self.new_member_id = id,
            Msg::NewMemberRole(role) => self.new_member_role = role,

            Msg::Error(msg) => {
                error("Error", msg);
            }
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
        let uuid_check = |value: &str| match value {
            "" => InputState::Default,
            _ => match uuid::Uuid::parse_str(value) {
                Err(_) => InputState::Error,
                _ => InputState::Default,
            },
        };

        let username_is_valid = uuid::Uuid::parse_str(self.new_member_id.as_str()).is_ok();

        if let Some(m) = &self.members {
            return html! {
                <PageSection>
                    <Card title={html!{"Application readers"}}>
                        <Table<SimpleTableModel<User>>
                                entries=SimpleTableModel::from(m.readers.clone())
                                    header={html_nested!{
                                        <TableHeader>
                                            <TableColumn label="User name"/>
                                            <TableColumn label="UserId"/>
                                        </TableHeader>
                                    }}
                                >
                        </Table<SimpleTableModel<User>>>
                    </Card>
                    <Card title={html!{"Application managers"}}>
                        <Table<SimpleTableModel<User>>
                            entries=SimpleTableModel::from(m.managers.clone())
                            header={html_nested!{
                                <TableHeader>
                                    <TableColumn label="User name"/>
                                    <TableColumn label="UserId"/>
                                </TableHeader>
                            }}
                        >
                        </Table<SimpleTableModel<User>>>
                    </Card>
                <Card>
                <Toolbar>
                    <ToolbarItem>
                        <TextInput
                                disabled=self.fetch.is_some()
                                validator=Validator::from(uuid_check)
                                onchange=self.link.callback(|id|Msg::NewMemberId(id))
                                placeholder="User id"/>
                        </ToolbarItem>
                        <ToolbarItem>
                            <Dropdown toggle={ html!{<DropdownToggle text=self.new_member_role.to_string()></DropdownToggle>}}>
                                <DropdownItem onclick=self.link.callback(|_|Msg::NewMemberRole(Role::Reader))>{"Reader"}</DropdownItem>
                                <Divider/>
                                <DropdownItem onclick=self.link.callback(|_|Msg::NewMemberRole(Role::Manager))>{"Manager"}</DropdownItem>
                                <Divider/>
                            </Dropdown>
                        </ToolbarItem>
                        <ToolbarItem>
                            <Button
                                    disabled=!username_is_valid
                                    label="Add"
                                    icon=Icon::PlusCircleIcon
                                    onclick=self.link.callback(|_|Msg::AddMember)
                            />
                    </ToolbarItem>
                </Toolbar>
                    <Form>
                        <ActionGroup>
                            <Button disabled=self.fetch.is_some() label="Save" variant=Variant::Primary onclick=self.link.callback(|_|Msg::SaveMembers)/>
                            <Button disabled=self.fetch.is_some() label="Reload" variant=Variant::Secondary onclick=self.link.callback(|_|Msg::LoadMembers)/>
                        </ActionGroup>
                    </Form>
                 </Card>
                // <Card title={html!{"Transfer Ownership"}}>
                    // TODO
                    // open a modal ? Or simply a form ?
                    // think about how we accept incoming requests.
                    // Need a new API call to list pending requests.
                // </Card>
                </PageSection>
            };
        } else {
            return html! {};
        }
    }
}

impl Admin {
    fn load(&self) -> Result<FetchTask, anyhow::Error> {
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
                    .into_body()
                    .0
                {
                    Ok(content) => Msg::SetMembers(content),
                    Err(err) => Msg::Error(err.to_string()),
                },
            ),
        )
    }

    fn submit(&self, members: &Members) -> Result<FetchTask, anyhow::Error> {
        let json_members = members;
        self.props.backend.info.request(
            Method::PUT,
            format!(
                "/api/admin/v1alpha1/apps/{}/members",
                url_encode(&self.props.name)
            ),
            Json(json_members),
            vec![("Content-Type", "application/json")],
            self.link
                .callback(move |response: Response<Text>| match response.status() {
                    status if status.is_success() => {
                        ToastDispatcher::default().toast(Toast {
                            title: "Success !".into(),
                            body: html! {<>
                                <Content>
                                <p>{"Application members saved."}</p>
                                </Content>
                            </>},
                            r#type: Type::Success,
                            timeout: Some(Duration::from_secs(3)),
                            actions: Vec::new(),
                        });
                        Msg::LoadMembers
                    }
                    status => Msg::Error(format!("Failed to perform update: {}", status)),
                }),
        )
    }
}

impl Users {
    pub fn from(members: Members, app: String, link: &ComponentLink<Admin>) -> Self {
        let mut readers: Vec<User> = Vec::new();
        let mut managers: Vec<User> = Vec::new();
        let mut admin = String::new();

        for (user, role) in members.members {
            match role.role {
                Role::Admin => admin = user,
                Role::Manager => managers.push(User {
                    id: user.clone(),
                    on_delete: link.callback(move |_| Msg::DeleteMember(user.clone())),
                }),
                Role::Reader => readers.push(User {
                    id: user.clone(),
                    on_delete: link.callback(move |_| Msg::DeleteMember(user.clone())),
                }),
            }
        }

        Users {
            app,
            managers,
            readers,
            admin,
            resource_version: members.resource_version.unwrap().clone(),
        }
    }

    //FIXME : remove the dependency on IndexMap by using generics for the Member struct :)
    pub fn serialize(&self) -> Members {
        let mut members: IndexMap<String, MemberEntry> = IndexMap::new();

        for u in &self.managers {
            members.insert(
                u.id.clone(),
                MemberEntry {
                    role: Role::Manager,
                },
            );
        }
        for u in &self.readers {
            members.insert(u.id.clone(), MemberEntry { role: Role::Reader });
        }
        members.insert(self.admin.clone(), MemberEntry { role: Role::Admin });

        Members {
            members,
            resource_version: Some(self.resource_version.clone()),
        }
    }

    pub fn delete(&mut self, id: String) {
        log::debug!("deleting {}", id);
        self.readers.retain(|u| *u.id != id);
        self.managers.retain(|u| *u.id != id);
        log::debug!("deleted ? {:?}", self.readers)
    }

    pub fn add(&mut self, id: String, role: Role, link: &ComponentLink<Admin>) -> Result<()> {
        let copy_id = id.clone();
        let user = User {
            id: id.clone(),
            on_delete: link.callback(move |_| Msg::DeleteMember(copy_id.clone())),
        };

        if self.contains(id.clone()) {
            Err(anyhow!("User is already a member"))
        } else {
            match role {
                Role::Reader => self.readers.push(user),
                Role::Manager => self.managers.push(user),
                // To change the admin, you should transfer the app.
                Role::Admin => {}
            }
            Ok(())
        }
    }

    pub fn contains(&self, id: String) -> bool {
        let user = &User {
            id: id.clone(),
            on_delete: Default::default(),
        };
        return self.readers.contains(user) || self.managers.contains(user) || self.admin == id;
    }
}
