use drogue_client::{core, dialect, Dialect, Section};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KafkaAppStatus {
    pub observed_generation: u64,
    pub conditions: core::v1::Conditions,
    pub downstream: Option<KafkaDownstreamStatus>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KafkaDownstreamStatus {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub topic: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bootstrap_servers: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

dialect!(KafkaAppStatus[Section::Status => "kafka"]);
