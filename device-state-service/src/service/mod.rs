mod config;
mod error;

pub use self::config::*;
pub use error::*;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use deadpool_postgres::{Pool, Transaction};
use drogue_cloud_database_common::{Client, DatabaseService};
use drogue_cloud_service_api::health::HealthChecked;
use serde::{Deserialize, Serialize};
use tokio_postgres::NoTls;
use uuid::Uuid;

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
    async fn create(&self, instance: String, id: String) -> Result<CreateResponse, ServiceError>;
    /// Delete the state of the item, for this session.
    ///
    /// If the state was already deleted, or belongs to a different session now, this becomes
    /// a no-op.
    async fn delete(&self, instance: String, id: String) -> Result<(), ServiceError>;
    /// Refresh the session timeout and retrieve lost items.
    ///
    /// If the "lost IDs" field was not empty, the caller should immediately re-ping, as more
    /// IDs might be waiting. This should be done until the list returns empty. This helps in
    /// limiting the response size.
    async fn ping(&self, instance: String) -> Result<PingResponse, ServiceError>;
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
    pub lost_ids: Vec<String>,
}

#[derive(Clone)]
pub struct PostgresDeviceStateService {
    pool: Pool,
    timeout: Duration,
}

impl PostgresDeviceStateService {
    pub fn new(config: PostgresServiceConfiguration) -> anyhow::Result<Self> {
        let pool = config.pg.create_pool(NoTls)?;
        Ok(Self {
            pool,
            timeout: Duration::seconds(10),
        })
    }

    #[doc(hidden)]
    pub fn for_testing(pool: Pool) -> Self {
        Self {
            pool,
            timeout: Duration::seconds(10),
        }
    }
}

impl DatabaseService for PostgresDeviceStateService {
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

impl HealthChecked for PostgresDeviceStateService {}

#[async_trait]
impl DeviceStateService for PostgresDeviceStateService {
    async fn init(&self) -> Result<InitResponse, ServiceError> {
        let c = self.pool.get().await?;

        let session = Uuid::new_v4();
        let now = Utc::now();

        c.execute(
            r#"
INSERT INTO
    sessions
(
    ID,
    LAST_PING
) VALUES (
    $1,
    $2
)"#,
            &[&session, &now],
        )
        .await?;

        Ok(InitResponse {
            session: session.to_string(),
        })
    }

    async fn create(&self, session: String, id: String) -> Result<CreateResponse, ServiceError> {
        let c = self.pool.get().await?;

        let r = c
            .query_opt(
                // FIXME: only insert when active session is available
                r#"
INSERT INTO
    states
(
    SESSION,
    ID
) VALUES (
    $1::text::uuid,
    $2
)
ON CONFLICT (ID)
    DO UPDATE
        SET lost = true
RETURNING
    LOST
"#,
                &[&session, &id],
            )
            .await?;

        match r {
            Some(row) => {
                let lost: bool = row.try_get("LOST")?;
                Ok(match lost {
                    false => CreateResponse::Created,
                    true => CreateResponse::Occupied,
                })
            }
            None => Err(ServiceError::NotInitialized),
        }
    }

    async fn delete(&self, session: String, id: String) -> Result<(), ServiceError> {
        let c = self.pool.get().await?;

        c.execute(
            r#"
DELETE FROM
    states
WHERE
        SESSION = $1::text::uuid
    AND
        ID = $2
"#,
            &[&session, &id],
        )
        .await?;

        Ok(())
    }

    async fn ping(&self, session: String) -> Result<PingResponse, ServiceError> {
        let c = self.pool.get().await?;

        let now = Utc::now();

        let r = c
            .execute(
                r#"
UPDATE
    sessions
SET
    LAST_PING = $2
WHERE
        ID = $1::text::uuid
    AND
        LAST_PING + $3::text::interval > $2
"#,
                &[
                    &session,
                    &now,
                    &format!("{}ms", self.timeout.num_milliseconds()),
                ],
            )
            .await?;

        if r > 0 {
            // TODO: consider using a LIMIT on the query
            let r = c
                .query(
                    r#"
SELECT
    ID
FROM
    states
WHERE
        SESSION = $1::text::uuid
    AND
        LOST = true
"#,
                    &[&session],
                )
                .await?;

            let lost_ids = r
                .into_iter()
                .map(|row| row.try_get::<_, String>("ID"))
                .collect::<Result<_, _>>()?;

            Ok(PingResponse { lost_ids })
        } else {
            Err(ServiceError::NotInitialized)
        }
    }
}

#[derive(Clone, Copy)]
pub struct Partitioner {
    pub index: u16,
    pub of: u16,
}

impl PostgresDeviceStateService {
    pub async fn prune(&self) -> Result<(), ServiceError> {
        let mut c = self.pool.get().await?;

        loop {
            let t = c.build_transaction().start().await?;
            let now = Utc::now();
            let next = match t
                .query_opt(
                    r#"
SELECT
    ID
FROM
    sessions
WHERE
    LAST_PING + $1::text::interval <= $2
ORDER BY
    LAST_PING ASC
LIMIT
    1
FOR UPDATE SKIP LOCKED
"#,
                    &[&format!("{}ms", self.timeout.num_milliseconds()), &now],
                )
                .await?
            {
                None => break,
                Some(next) => next,
            };

            self.prune_session(t, next.try_get("ID")?).await?;
        }

        Ok(())
    }

    async fn prune_session(&self, t: Transaction<'_>, id: Uuid) -> Result<(), ServiceError> {
        log::info!("Pruning session: {id}");

        // FIXME: implement sending out events

        t.execute(
            r#"
DELETE FROM
    sessions
WHERE
    id = $1
"#,
            &[&id],
        )
        .await?;

        t.commit().await?;

        Ok(())
    }
}
