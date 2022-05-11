use crate::{error::ServiceError, Client};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use futures::{Stream, StreamExt};
use std::{
    convert::{TryFrom, TryInto},
    pin::Pin,
};
use tokio_postgres::{types::Type, Row};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboxEntry {
    pub instance: String,

    pub app: String,
    pub device: Option<String>,
    pub path: String,

    pub revision: u64,
    pub uid: String,
}

impl TryFrom<Row> for OutboxEntry {
    type Error = ServiceError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        Ok(OutboxEntry {
            instance: row.try_get("INSTANCE")?,
            app: row.try_get("APP")?,
            device: {
                let id: String = row.try_get("DEVICE")?;
                if id.is_empty() {
                    None
                } else {
                    Some(id)
                }
            },
            path: row.try_get("PATH")?,
            revision: row.try_get::<_, i64>("REVISION")? as u64,
            uid: row.try_get("UID")?,
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
        let sql = r#"
INSERT INTO outbox (
    INSTANCE,
    APP,
    DEVICE,
    UID,
    PATH,
    REVISION,
    TS
) VALUES (
    $1,
    $2,
    $3,
    $4,
    $5,
    $6,
    now()
) 
ON CONFLICT (APP, DEVICE, PATH) 
DO
    UPDATE SET
        REVISION = EXCLUDED.REVISION,
        UID = EXCLUDED.UID,
        TS = EXCLUDED.TS
    WHERE
            outbox.REVISION < EXCLUDED.REVISION
        OR
            outbox.UID != EXCLUDED.UID
"#;

        let stmt = self
            .client
            .prepare_typed(
                sql,
                &[
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::INT8,
                ],
            )
            .await?;

        let num = self
            .client
            .execute(
                &stmt,
                &[
                    &entry.instance,
                    &entry.app,
                    &entry.device.unwrap_or_default(),
                    &entry.uid,
                    &entry.path,
                    &(entry.revision as i64),
                ],
            )
            .await?;

        log::debug!("Rows changed by create: {}", num);

        Ok(())
    }

    async fn mark_seen(&self, entry: OutboxEntry) -> Result<bool, ServiceError> {
        // We do not filter by instance here, as we expect to own the full table, and
        // don't add the extra data to the index this way.

        let sql = r#"
DELETE
    FROM outbox
WHERE
        APP = $1
    AND
        DEVICE = $2
    AND
        PATH = $3
    AND
        REVISION <= $4
    AND
        UID = $5
"#;

        let stmt = self
            .client
            .prepare_typed(
                sql,
                &[
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::VARCHAR,
                    Type::INT8,
                    Type::VARCHAR,
                ],
            )
            .await?;

        let num = self
            .client
            .execute(
                &stmt,
                &[
                    &entry.app,
                    &entry.device.unwrap_or_default(),
                    &entry.path,
                    &(entry.revision as i64),
                    &entry.uid,
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

        let sql = r#"
SELECT
    INSTANCE, APP, DEVICE, PATH, REVISION, UID
FROM
    outbox
WHERE
    TS < $1
ORDER BY
    TS ASC
"#;

        let stmt = self.client.prepare_typed(sql, &[Type::TIMESTAMPTZ]).await?;

        let result = self.client.query_raw(&stmt, &[beginning]).await?;

        Ok(Box::pin(result.map(|item| match item {
            Ok(row) => row.try_into(),
            Err(err) => Err(err.into()),
        })))
    }
}
