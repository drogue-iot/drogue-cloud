use drogue_client::core::v1::Conditions;
use drogue_client::{core, dialect, Section};
use drogue_cloud_operator_common::controller::base::StatusSection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DittoAppStatus {
    pub observed_generation: u64,
    pub conditions: core::v1::Conditions,
}

dialect!(DittoAppStatus[Section::Status => "ditto"]);

const CONDITION_DITTO_READY: &str = "DittoReady";

impl StatusSection for DittoAppStatus {
    fn ready_name() -> &'static str {
        CONDITION_DITTO_READY
    }

    fn update_status(&mut self, conditions: Conditions, observed_generation: u64) {
        self.conditions = conditions;
        self.observed_generation = observed_generation;
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DittoDeviceStatus {
    pub observed_generation: u64,
    pub conditions: core::v1::Conditions,
}

dialect!(DittoDeviceStatus[Section::Status => "ditto"]);

impl StatusSection for DittoDeviceStatus {
    fn ready_name() -> &'static str {
        CONDITION_DITTO_READY
    }

    fn update_status(&mut self, conditions: Conditions, observed_generation: u64) {
        self.conditions = conditions;
        self.observed_generation = observed_generation;
    }
}
