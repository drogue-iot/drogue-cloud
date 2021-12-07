use super::EntityId;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_with::{serde_as, DisplayFromStr};
use std::{
    fmt::{Debug, Display, Formatter},
    str::FromStr,
};

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    #[serde_as(as = "DisplayFromStr")]
    pub policy_id: EntityId,
    pub entries: IndexMap<String, PolicyEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyEntry {
    pub subjects: IndexMap<String, Subject>,
    pub resources: IndexMap<Resource, Permissions>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    pub r#type: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    // must be present, even if empty
    #[serde(default)]
    pub grant: Vec<Permission>,
    // must be present, even if empty
    #[serde(default)]
    pub revoke: Vec<Permission>,
}

impl Permissions {
    pub fn grant<I>(permissions: I) -> Self
    where
        I: IntoIterator<Item = Permission>,
    {
        Self {
            grant: permissions.into_iter().collect(),
            revoke: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Permission {
    Read,
    Write,
    Execute,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub enum Resource {
    Thing(String),
    Policy(String),
    Message(String),
}

#[allow(unused)]
impl Resource {
    pub fn thing<S: Into<String>>(path: S) -> Self {
        Self::Thing(path.into())
    }
    pub fn policy<S: Into<String>>(path: S) -> Self {
        Self::Policy(path.into())
    }
    pub fn message<S: Into<String>>(path: S) -> Self {
        Self::Message(path.into())
    }
}

impl Serialize for Resource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Display for Resource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Resource::Thing(path) => write!(f, "thing:{}", path),
            Resource::Policy(path) => write!(f, "policy:{}", path),
            Resource::Message(path) => write!(f, "message:{}", path),
        }
    }
}

impl FromStr for Resource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split(':').collect::<Vec<_>>()[..] {
            ["thing", path] => Ok(Self::Thing(path.into())),
            ["policy", path] => Ok(Self::Policy(path.into())),
            ["message", path] => Ok(Self::Message(path.into())),
            _ => Err("Invalid policy ID".into()),
        }
    }
}

#[allow(unused)]
pub enum PolicyOperation {
    Create(Policy),
    Update(Policy),
    Delete(EntityId),
}

/*
impl<'de> protocol::Request<'de> for PolicyOperation {
    type Options = IndexMap<String, String>;
    type Response = Value;

    fn request(self, options: Self::Options) -> protocol::RequestEnvelope<Self> {
        let topic = format!(
            "{}/{}/policies/commands/create",
            self.policy_id.0, self.policy_id.1
        );
        protocol::RequestEnvelope {
            topic,
            path: "/".to_string(),
            value: self,
            options: protocol::RequestOptions {
                headers: options,
                ..Default::default()
            },
        }
    }
}
 */

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_1() {
        assert_eq!(
            serde_json::to_value(Policy {
                policy_id: ("ns", "policy").into(),
                entries: {
                    let mut map = IndexMap::new();
                    map.insert(
                        "FOO".to_string(),
                        PolicyEntry {
                            subjects: {
                                let mut map = IndexMap::new();
                                map.insert(
                                    "some:subject".to_string(),
                                    Subject {
                                        r#type: "foo".into(),
                                    },
                                );
                                map
                            },
                            resources: {
                                let mut map = IndexMap::new();
                                map.insert(
                                    Resource::Thing("/foo".to_string()),
                                    Permissions {
                                        ..Default::default()
                                    },
                                );
                                map
                            },
                        },
                    );
                    map
                },
            })
            .unwrap(),
            json!({
                "policyId": "ns:policy",
                "entries": {
                    "FOO": {
                        "subjects": {"some:subject": {"type": "foo"}}
                    }
                },
                "resources": {"thing:/foo": {}}
            })
        )
    }
}
