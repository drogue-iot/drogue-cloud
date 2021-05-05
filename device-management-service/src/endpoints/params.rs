use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteParams {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preconditions: Option<Preconditions>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preconditions {
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub uid: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub resource_version: String,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelector {
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub labels: String,
}
