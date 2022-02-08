use drogue_client::{
    core::{self, v1::Conditions},
    dialect, Section,
};
use drogue_cloud_operator_common::controller::base::StatusSection;
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DittoAppSpec {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exporter: Option<Exporter>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingress: Option<Ingress>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ingress {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clients: Option<u32>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumers: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exporter {
    pub kafka: KafkaClientConfig,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<ExporterTarget>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExporterTarget {
    pub topic: String,

    pub mode: ExporterMode,

    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<DittoTopic>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DittoTopic {
    #[serde(rename_all = "camelCase")]
    TwinEvents {
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        extra_fields: Vec<String>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        filter: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExporterMode {
    Ditto {
        #[serde(default)]
        normalized: bool,
    },
    #[serde(rename_all = "camelCase")]
    CloudEvents {
        #[serde(default)]
        normalized: bool,
    },
}

impl Default for ExporterMode {
    fn default() -> Self {
        Self::CloudEvents { normalized: true }
    }
}

dialect!(DittoAppSpec[Section::Spec => "ditto"]);

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
