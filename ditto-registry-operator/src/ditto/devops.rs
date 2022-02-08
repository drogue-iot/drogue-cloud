use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevopsCommand {
    pub target_actor_selection: String,
    pub headers: Headers,
    pub piggyback_command: PiggybackCommand,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityResponse {
    pub connectivity: IndexMap<String, ConnectivityResponseEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityResponseEntry {
    pub status: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceResponse {
    #[serde(rename = "?")]
    pub entries: IndexMap<String, NamespaceResponseEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceResponseEntry {
    pub r#type: String,
    pub status: u32,
    pub namespace: String,
    pub resource_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successful: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Headers {
    pub aggregate: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_group_topic: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PiggybackCommand {
    #[serde(rename = "connectivity.commands:createConnection")]
    #[serde(rename_all = "camelCase")]
    CreateConnection { connection: Connection },
    #[serde(rename = "connectivity.commands:modifyConnection")]
    #[serde(rename_all = "camelCase")]
    ModifyConnection { connection: Connection },
    #[serde(rename = "connectivity.commands:deleteConnection")]
    #[serde(rename_all = "camelCase")]
    DeleteConnection { connection_id: String },
    #[serde(rename = "namespaces.commands:blockNamespace")]
    #[serde(rename_all = "camelCase")]
    BlockNamespace { namespace: String },
    #[serde(rename = "common.commands:shutdown")]
    #[serde(rename_all = "camelCase")]
    Shutdown { reason: ShutdownReason },
    #[serde(rename = "namespaces.commands:purgeNamespace")]
    #[serde(rename_all = "camelCase")]
    PurgeNamespace { namespace: String },
    #[serde(rename = "namespaces.commands:unblockNamespace")]
    #[serde(rename_all = "camelCase")]
    UnblockNamespace { namespace: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
pub enum ShutdownReason {
    PurgeNamespace {
        #[serde(rename = "details")]
        namespace: String,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub connection_type: String,
    pub connection_status: ConnectionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_count: Option<u32>,
    pub failover_enabled: bool,
    pub uri: String,
    #[serde(default = "default_validate_certificates")]
    pub validate_certificates: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca: Option<String>,
    pub specific_config: IndexMap<String, String>,
    pub sources: Vec<Source>,
    pub targets: Vec<Target>,
    pub mapping_definitions: IndexMap<String, MappingDefinition>,
}

const fn default_validate_certificates() -> bool {
    true
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionStatus {
    Open,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Open
    }
}

#[derive(Clone, Copy, Debug, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub addresses: Vec<String>,
    pub consumer_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qos: Option<QoS>,
    pub authorization_context: Vec<String>,
    pub enforcement: Enforcement,
    pub header_mapping: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload_mapping: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub address: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
    pub authorization_context: Vec<String>,
    pub header_mapping: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload_mapping: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingDefinition {
    pub mapping_engine: String,
    pub options: IndexMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Enforcement {
    pub input: String,
    pub filters: Vec<String>,
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::*;

    #[test]
    fn test_serialize_delete() {
        let cmd = DevopsCommand {
            target_actor_selection: "/system/sharding/connection".to_string(),
            headers: Default::default(),
            piggyback_command: PiggybackCommand::DeleteConnection {
                connection_id: "connection-id".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_value(cmd).unwrap(),
            json!({
              "targetActorSelection": "/system/sharding/connection",
              "headers": {
                "aggregate": false
              },
              "piggybackCommand": {
                "type": "connectivity.commands:deleteConnection",
                "connectionId": "connection-id"
              }
            })
        );
    }

    #[test]
    fn test_serialize_shutdown() {
        let cmd = DevopsCommand {
            target_actor_selection: "/foo/bar".to_string(),
            headers: Default::default(),
            piggyback_command: PiggybackCommand::Shutdown {
                reason: ShutdownReason::PurgeNamespace {
                    namespace: "ns".to_string(),
                },
            },
        };

        assert_eq!(
            serde_json::to_value(cmd).unwrap(),
            json!({
                "targetActorSelection": "/foo/bar",
                "headers": {
                    "aggregate": false
                },
                "piggybackCommand": {
                    "type": "common.commands:shutdown",
                    "reason": {
                        "type": "purge-namespace",
                        "details": "ns",
                    }
                }
            })
        );
    }
}
