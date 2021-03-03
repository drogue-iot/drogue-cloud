use async_trait::async_trait;
use chrono::Duration;
use deadpool_postgres::Pool;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_database_common::models::outbox::{
    OutboxAccessor, OutboxEntry, PostgresOutboxAccessor,
};
use drogue_cloud_database_common::DatabaseService;
use drogue_cloud_registry_events::Event;
use drogue_cloud_service_api::health::HealthCheckedService;
use drogue_cloud_service_common::config::ConfigFromEnv;
use futures::Stream;
use serde::Deserialize;
use std::pin::Pin;
use tokio_postgres::NoTls;

#[derive(Clone, Debug, Deserialize)]
pub struct OutboxServiceConfig {
    pub pg: deadpool_postgres::Config,
}

/// A service for interacting with the outbox, mark entries seen and re-deliver lost ones.
#[derive(Clone)]
pub struct OutboxService {
    pool: Pool,
}

impl<'de> ConfigFromEnv<'de> for OutboxServiceConfig {}

impl DatabaseService for OutboxService {
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait]
impl HealthCheckedService for OutboxService {
    type HealthCheckError = ServiceError;

    async fn is_ready(&self) -> Result<(), Self::HealthCheckError> {
        (self as &dyn DatabaseService).is_ready().await
    }
}

impl OutboxService {
    pub fn new(config: OutboxServiceConfig) -> anyhow::Result<Self> {
        Ok(Self {
            pool: config.pg.create_pool(NoTls)?,
        })
    }

    pub async fn mark_seen(&self, event: Event) -> Result<(), ServiceError> {
        let c = self.pool.get().await?;

        let outbox = PostgresOutboxAccessor::new(&c);

        outbox.mark_seen(event.into()).await?;

        Ok(())
    }

    pub async fn retrieve_unseen(
        &self,
        before: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<OutboxEntry, ServiceError>>>>, ServiceError> {
        let c = self.pool.get().await?;

        let outbox = PostgresOutboxAccessor::new(&c);

        outbox.fetch_unread(before).await
    }
}
