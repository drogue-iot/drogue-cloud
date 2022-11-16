mod error;

pub mod postgres;

pub use error::*;

use async_trait::async_trait;
use drogue_client::{error::ClientError, registry};
use drogue_cloud_service_api::services::command_routing::*;
use serde::Deserialize;

#[async_trait]
pub trait CommandRoutingService: Send + Sync {
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
        token: String,
        state: CommandRoute,
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
        token: String,
        opts: DeleteOptions,
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
    ) -> Result<Option<CommandRouteResponse>, ServiceError>;
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
