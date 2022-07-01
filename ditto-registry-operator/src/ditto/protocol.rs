//! The Ditto WS protocol mapping

use crate::ditto::data::*;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

#[allow(unused)]
pub enum PolicyOperation {
    Create(Policy),
    Update(Policy),
    Delete(EntityId),
}

#[allow(unused)]
pub enum ThingOperation {
    CreateOrUpdate(Box<Thing>),
    Delete(EntityId),
}

pub trait ToTopic {
    fn to_topic(&self, path: &str) -> String;
}

impl ToTopic for EntityId {
    fn to_topic(&self, path: &str) -> String {
        format!("{}/{}/{}", self.0, self.1, path)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope<T>
where
    T: Clone + Debug + Serialize,
{
    pub topic: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,
    #[serde(flatten)]
    pub options: RequestOptions,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub headers: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub extra: IndexMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
}

pub trait Request {
    type Request: Serialize + Clone + Debug;
    type Response: for<'de> Deserialize<'de>;
    fn request(self, options: RequestOptions) -> RequestEnvelope<Self::Request>;
}

impl<T> Deref for RequestEnvelope<T>
where
    T: Clone + Debug + Serialize,
{
    type Target = RequestOptions;

    fn deref(&self) -> &Self::Target {
        &self.options
    }
}

impl<T> DerefMut for RequestEnvelope<T>
where
    T: Clone + Debug + Serialize,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.options
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope<T>
where
    T: Clone + Debug,
{
    pub topic: String,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub headers: IndexMap<String, String>,
    pub path: String,
    pub value: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
    pub status: u32,
}

impl Request for PolicyOperation {
    type Request = Policy;
    type Response = Policy;

    fn request(self, options: RequestOptions) -> RequestEnvelope<Self::Request> {
        match self {
            Self::Create(policy) => RequestEnvelope {
                topic: policy.policy_id.to_topic("policies/commands/create"),
                path: "/".to_string(),
                value: Some(policy),
                options,
            },
            Self::Update(policy) => RequestEnvelope {
                topic: policy.policy_id.to_topic("policies/commands/modify"),
                path: "/".to_string(),
                value: Some(policy),
                options,
            },
            Self::Delete(policy_id) => RequestEnvelope {
                topic: policy_id.to_topic("policies/commands/delete"),
                path: "/".to_string(),
                value: None,
                options,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ditto::data::Policy;
    use serde_json::json;

    #[test]
    fn test_2() {
        assert_eq!(
            serde_json::to_value(
                PolicyOperation::Create(Policy {
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
                .request(Default::default())
            )
            .unwrap(),
            json!({
                "topic": "ns/policy/policies/commands/create",
                "path": "/",
                "value": {
                    "policyId": "ns:policy",
                    "entries": {
                        "FOO": {
                            "subjects": {
                                "some:subject": {"type": "foo"}
                            },
                            "resources": {
                                "thing:/foo": {
                                    "grant": [],
                                    "revoke": [],
                                },
                            }
                        }
                    },
                }
            })
        )
    }
}
