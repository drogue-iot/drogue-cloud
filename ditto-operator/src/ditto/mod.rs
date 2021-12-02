mod client;

pub use client::*;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevopsCommand {
    pub target_actor_selection: String,
    pub headers: Headers,
    pub piggyback_command: PiggybackCommand,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Headers {
    pub aggregate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PiggybackCommand {
    #[serde(rename = "connectivity.commands:createConnection")]
    #[serde(rename_all = "camelCase")]
    CreateConnection { connection: Connection },
    #[serde(rename = "connectivity.commands:deleteConnection")]
    #[serde(rename_all = "camelCase")]
    DeleteConnection { connection_id: String },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub connection_type: String,
    pub connection_status: ConnectionStatus,
    pub failover_enabled: bool,
    pub uri: String,
    pub specific_config: IndexMap<String, String>,
    pub sources: Vec<Source>,
    pub targets: Vec<Target>,
    pub mapping_definitions: IndexMap<String, MappingDefinition>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionStatus {
    Open,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Open
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub addresses: Vec<String>,
    pub consumer_count: u32,
    pub authorization_context: Vec<String>,
    pub enforcement: Enforcement,
    pub header_mapping: IndexMap<String, String>,
    pub payload_mapping: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingDefinition {
    pub mapping_engine: String,
    pub options: IndexMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    fn test_serialize() -> anyhow::Result<()> {
        let cmd = DevopsCommand {
            target_actor_selection: "/system/sharding/connection".to_string(),
            headers: Default::default(),
            piggyback_command: PiggybackCommand::DeleteConnection {
                connection_id: "connection-id".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_value(cmd)?,
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

        Ok(())
    }
}
