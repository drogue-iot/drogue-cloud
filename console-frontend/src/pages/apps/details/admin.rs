use drogue_cloud_service_api::auth::user::authz::Permission;

use patternfly_yew::*;
use yew::prelude::*;
use yew::{Html, *};

use drogue_client::registry::v1::Application;
use drogue_cloud_admin_service::apps::{Members, Role};

pub struct Admin {
    managers: Vec<User>,
    readers: Vec<User>,
    admin: String,
    resource_version: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct User(String);

impl TableRenderer for User {
    fn render(&self, column: ColumnIndex) -> Html {
        match column.index {
            0 => self.clone().0.into(),
            1 => self.clone().0.into(),
            _ => html! {},
        }
    }

    // fn actions(&self) -> Vec<DropdownChildVariant> {
    //     vec![html_nested! {
    //     <DropdownItem
    //         onclick=self.on_delete.clone()
    //     >
    //         {"Delete"}
    //     </DropdownItem>}
    //         .into()]
    // }
}

impl Admin {
    pub fn from(members: Members) -> Self {
        let mut readers: Vec<User> = Vec::new();
        let mut managers: Vec<User> = Vec::new();
        let mut admin = String::new();

        for (user, role) in members.members {
            match role.role {
                Role::Admin => admin = user,
                Role::Manager => managers.push(User(user)),
                Role::Reader => readers.push(User(user)),
            }
        }

        Admin {
            managers,
            readers,
            admin,
            resource_version: members.resource_version.unwrap().clone(),
        }
    }

    pub fn render(&self) -> Html {
        return html! {
            <PageSection>
                <Card title={html!{"Application readers"}}>
                    <Table<SimpleTableModel<User>>
                        entries=SimpleTableModel::from(self.readers.clone())
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
                        entries=SimpleTableModel::from(self.managers.clone())
                        header={html_nested!{
                                            <TableHeader>
                                                <TableColumn label="User name"/>
                                                <TableColumn label="UserId"/>
                                            </TableHeader>
                                        }}
                    >
                    </Table<SimpleTableModel<User>>>
                </Card>
            </PageSection>
        };
    }
}
