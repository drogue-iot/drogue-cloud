use drogue_client::{core, dialect, Dialect, Section};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KafkaAppStatus {
    pub observed_generation: u64,
    pub conditions: core::v1::Conditions,
}

dialect!(KafkaAppStatus[Section::Status => "kafka"]);
