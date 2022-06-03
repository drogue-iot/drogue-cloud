use chrono::Duration;
use deadpool::Runtime;
use deadpool_postgres::Pool;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_database_common::models::outbox::{
    OutboxAccessor, OutboxEntry, PostgresOutboxAccessor,
};
use drogue_cloud_database_common::DatabaseService;
use drogue_cloud_registry_events::Event;
use drogue_cloud_service_api::health::{HealthCheckError, HealthChecked};
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

impl DatabaseService for OutboxService {
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait::async_trait]
impl HealthChecked for OutboxService {
    async fn is_ready(&self) -> Result<(), HealthCheckError> {
        Ok(DatabaseService::is_ready(self)
            .await
            .map_err(HealthCheckError::from)?)
    }
}

impl OutboxService {
    pub fn new(config: OutboxServiceConfig) -> anyhow::Result<Self> {
        Ok(Self {
            pool: config.pg.create_pool(Some(Runtime::Tokio1), NoTls)?,
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
