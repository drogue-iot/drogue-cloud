use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const CONNECTION_TYPE_EVENT: &str = "io.drogue.connection.v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEvent {
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Id {
    pub application: String,
    pub device: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRoute {
    pub device_uid: String,
    pub endpoint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastWillTestament {
    pub channel: String,
    pub payload: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRouteResponse {
    pub state: CommandRoute,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitResponse {
    pub session: String,
    pub expires: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    pub token: String,
    pub state: CommandRoute,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreateResponse {
    // State was created.
    Created,
    // Device state is still occupied.
    Occupied,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub expires: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost_ids: Vec<Id>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequest {
    pub token: String,
}
