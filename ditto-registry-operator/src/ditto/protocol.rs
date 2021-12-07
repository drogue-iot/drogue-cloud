//! The Ditto WS protocol mapping

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope<T>
where
    T: Clone + Debug + Serialize,
{
    pub topic: String,
    pub path: String,
    pub value: T,
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

pub trait Request<'de>: Clone + Debug + Serialize + Deserialize<'de> {
    type Options;
    type Response;
    fn request(self, options: Self::Options) -> RequestEnvelope<Self>;
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

/*
#[cfg(test)]
mod test {
    use super::*;
    use crate::ditto::messages::*;
    use serde_json::json;

    #[test]
    fn test_2() {
        assert_eq!(
            serde_json::to_value(
                Policy {
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
                }
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
                            "subjects": {"some:subject": {"type": "foo"}}
                        }
                    },
                    "resources": {"thing:/foo": {}}
                }
            })
        )
    }
}
*/
