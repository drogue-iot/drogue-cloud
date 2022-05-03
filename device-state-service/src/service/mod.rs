mod config;
mod error;

pub mod postgres;

pub use self::config::*;
pub use error::*;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use drogue_client::{error::ClientError, registry};
use serde::{Deserialize, Serialize};

pub const CONNECTION_TYPE_EVENT: &str = "io.drogue.connection.v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEvent {
    pub connected: bool,
}

#[async_trait]
pub trait DeviceStateService: Send + Sync {
    /// Create a new session.
    async fn init(&self) -> Result<InitResponse, ServiceError>;

    /// Create a new state.
    ///
    /// The outcome might be either [`CreateResponse::Created`], in which case there wasn't yet
    /// a state for the ID. If there already is a state, then [`CreateResponse::Occupied`] will
    /// be returned. This automatically marks the entry as "lost" (to the other session). The next
    /// ping for the session will return the ID.
    ///
    /// **NOTE:** Even re-creating a state for the same session will mark it as lost.
    async fn create(
        &self,
        instance: String,
        application: String,
        device: String,
        state: DeviceState,
    ) -> Result<CreateResponse, ServiceError>;

    /// Delete the state of the item, for this session.
    ///
    /// If the state was already deleted, or belongs to a different session now, this becomes
    /// a no-op.
    async fn delete(
        &self,
        instance: String,
        application: String,
        device: String,
    ) -> Result<(), ServiceError>;

    /// Refresh the session timeout and retrieve lost items.
    ///
    /// If the "lost IDs" field was not empty, the caller should immediately re-ping, as more
    /// IDs might be waiting. This should be done until the list returns empty. This helps in
    /// limiting the response size.
    async fn ping(&self, instance: String) -> Result<PingResponse, ServiceError>;

    /// Get the current state of a device.
    async fn get(
        &self,
        application: String,
        device: String,
    ) -> Result<Option<DeviceStateResponse>, ServiceError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Id {
    pub application: String,
    pub device: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceState {
    pub device_uid: String,
    pub endpoint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStateResponse {
    pub created: DateTime<Utc>,
    pub state: DeviceState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitResponse {
    pub session: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost_ids: Vec<Id>,
}

#[async_trait]
pub trait ApplicationLookup: Send + Sync {
    async fn lookup(
        &self,
        application: &str,
    ) -> Result<Option<registry::v1::Application>, ClientError>;
}

#[async_trait]
impl ApplicationLookup for registry::v1::Client {
    async fn lookup(
        &self,
        application: &str,
    ) -> Result<Option<registry::v1::Application>, ClientError> {
        self.get_app(application).await
    }
}

#[async_trait]
impl ApplicationLookup for std::collections::HashMap<String, registry::v1::Application> {
    async fn lookup(
        &self,
        application: &str,
    ) -> Result<Option<registry::v1::Application>, ClientError> {
        Ok(self.get(application).cloned())
    }
}
