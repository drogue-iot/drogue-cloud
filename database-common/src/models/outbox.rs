use crate::{error::ServiceError, Client};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use futures::{Stream, StreamExt};
use std::convert::{TryFrom, TryInto};
use std::pin::Pin;
use tokio_postgres::Row;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboxEntry {
    pub instance_id: String,

    pub app_id: String,
    pub device_id: Option<String>,
    pub path: String,

    pub generation: u64,
}

impl TryFrom<Row> for OutboxEntry {
    type Error = ServiceError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        Ok(OutboxEntry {
            instance_id: row.try_get("INSTANCE_ID")?,
            app_id: row.try_get("APP_ID")?,
            device_id: {
                let id: String = row.try_get("DEVICE_ID")?;
                if id.is_empty() {
                    None
                } else {
                    Some(id)
                }
            },
            path: row.try_get("PATH")?,
            generation: row.try_get::<_, i64>("GENERATION")? as u64,
        })
    }
}

#[async_trait]
pub trait OutboxAccessor {
    /// Create a new outbox entry.
    async fn create(&self, entry: OutboxEntry) -> Result<(), ServiceError>;
    /// Mark the outbox entry as seen.
    async fn mark_seen(&self, entry: OutboxEntry) -> Result<bool, ServiceError>;
    /// Fetch unread.
    ///
    /// This will return a stream of entries which got created `before`. The stream is ordered by
    /// creation timestamp (ascending, oldest entries first).
    async fn fetch_unread(
        &self,
        before: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<OutboxEntry, ServiceError>>>>, ServiceError>;
}

pub struct PostgresOutboxAccessor<'c, C: Client> {
    client: &'c C,
}

impl<'c, C: Client> PostgresOutboxAccessor<'c, C> {
    pub fn new(client: &'c C) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<'c, C: Client> OutboxAccessor for PostgresOutboxAccessor<'c, C> {
    async fn create(&self, entry: OutboxEntry) -> Result<(), ServiceError> {
        let num = self
            .client
            .execute(
                r#"
INSERT INTO outbox (
    INSTANCE_ID,
    APP_ID,
    DEVICE_ID,
    PATH,
    GENERATION,
    TS
) VALUES (
    $1,
    $2,
    $3,
    $4,
    $5,
    now()
) 
ON CONFLICT (APP_ID, DEVICE_ID, PATH) 
DO
    UPDATE SET
        GENERATION = EXCLUDED.GENERATION,
        TS = EXCLUDED.TS
    WHERE
        outbox.GENERATION < EXCLUDED.GENERATION;
"#,
                &[
                    &entry.instance_id,
                    &entry.app_id,
                    &entry.device_id.unwrap_or_default(),
                    &entry.path,
                    &(entry.generation as i64),
                ],
            )
            .await?;

        log::debug!("Rows changed by create: {}", num);

        Ok(())
    }

    async fn mark_seen(&self, entry: OutboxEntry) -> Result<bool, ServiceError> {
        let num = self
            .client
            .execute(
                r#"
DELETE
    FROM outbox
WHERE
        APP_ID = $1
    AND
        DEVICE_ID = $2
    AND
        PATH = $3
    AND
        GENERATION <= $4
"#,
                &[
                    &entry.app_id,
                    &entry.device_id.unwrap_or_default(),
                    &entry.path,
                    &(entry.generation as i64),
                ],
            )
            .await?;

        log::debug!("Rows deleted: {}", num);

        Ok(num > 0)
    }

    async fn fetch_unread(
        &self,
        duration: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<OutboxEntry, ServiceError>>>>, ServiceError> {
        let beginning = Utc::now() - duration;

        let result = self
            .client
            .query_raw(
                r#"
SELECT
    INSTANCE_ID, APP_ID, DEVICE_ID, PATH, GENERATION
FROM
    outbox
WHERE
    TS < $1
ORDER BY
    TS ASC
"#,
                &[beginning],
            )
            .await?;

        Ok(Box::pin(result.map(|item| match item {
            Ok(row) => row.try_into(),
            Err(err) => Err(err.into()),
        })))
    }
}
