pub use drogue_cloud_database_common::models::app::Role;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferOwnership {
    pub new_user: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Members {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub members: IndexMap<String, MemberEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemberEntry {
    pub role: Role,
}
