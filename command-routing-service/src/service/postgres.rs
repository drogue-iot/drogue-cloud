use super::*;

use async_trait::async_trait;
use chrono::Utc;
use deadpool_postgres::{Pool, Transaction};
use drogue_cloud_database_common::{postgres, Client, DatabaseService};
use drogue_cloud_service_api::health::HealthChecked;
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use tokio_postgres::{
    types::{Json, Type},
};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize)]
pub struct PostgresServiceConfiguration {
    #[serde(with = "humantime_serde", default = "default_session_timeout")]
    pub session_timeout: Duration,
    pub pg: postgres::Config,
}

const fn default_session_timeout() -> Duration {
    Duration::from_secs(10)
}

#[derive(Clone)]
pub struct PostgresCommandRoutingService {
    pool: Pool,
    registry: Arc<dyn ApplicationLookup>,
    timeout: chrono::Duration,
}

impl PostgresCommandRoutingService {
    pub fn new(
        config: PostgresServiceConfiguration,
        registry: impl ApplicationLookup + 'static,
    ) -> anyhow::Result<Self> {
        let pool = config.pg.create_pool()?;

        let timeout = chrono::Duration::from_std(config.session_timeout)?;

        Ok(Self {
            pool,
            registry: Arc::new(registry),
            timeout,
        })
    }
}

impl DatabaseService for PostgresCommandRoutingService {
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

impl HealthChecked for PostgresCommandRoutingService {}

#[async_trait]
impl CommandRoutingService for PostgresCommandRoutingService {
    async fn init(&self) -> Result<InitResponse, ServiceError> {
        let c = self.pool.get().await?;

        let session = Uuid::new_v4();
        let url = "http://localhost:10009";
        let now = Utc::now();

        c.execute(
            r#"
INSERT INTO
    command_sessions
(
    ID,
    SESSION_URL,
    LAST_PING
) VALUES (
    $1,
    $2,
    $3
)"#,
            &[&session, &url, &now],
        )
        .await?;

        Ok(InitResponse {
            session: session.to_string(),
            expires: now + self.timeout,
        })
    }

    async fn create(
        &self,
        session: String,
        application: String,
        device: String,
        _token: String,
        _state: CommandRoute,
    ) -> Result<CreateResponse, ServiceError> {
        let _app = match self.registry.lookup(&application).await? {
            Some(app) => app,
            None => {
                log::info!("Application not found: {application}");
                return Err(ServiceError::ApplicationNotFound);
            }
        };

        let c = self.pool.get().await?;

        let _now = Utc::now();

        let command = "*".to_string();

        let r = c
            .execute(
                r#"
INSERT INTO
    command_routes
(
    SESSION,
    APPLICATION,
    DEVICE,
    COMMAND
) VALUES (
    $1::text::uuid,
    $2,
    $3,
    $4
)"#,
                &[&session, &application, &device, &command],
            )
            .await?;

        if r > 0 {
            Ok(CreateResponse::Created)
        } else {
            Ok(CreateResponse::Occupied)
        }
    }

    async fn delete(
        &self,
        session: String,
        application: String,
        device: String,
    ) -> Result<(), ServiceError> {
        let c = self.pool.get().await?;

        c.execute(
                r#"
DELETE FROM
    command_routes
WHERE
        SESSION = $1::text::uuid
    AND
        APPLICATION = $2
    AND
        DEVICE = $3
"#,
                &[&session, &application, &device],
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
    command_sessions
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

            let lost_ids = Vec::new();
            Ok(PingResponse {
                expires: now + self.timeout,
                lost_ids,
            })
        } else {
            Err(ServiceError::NotInitialized)
        }
    }

    async fn get(
        &self,
        application: String,
        device: String,
    ) -> Result<Option<CommandRouteResponse>, ServiceError> {
        let c = self.pool.get().await?;

        let stmt = c
            .prepare_typed(
                r#"
SELECT CREATED, LOST, DATA FROM
    states
WHERE
        APPLICATION = $1
    AND
        DEVICE = $2
"#,
                &[Type::VARCHAR, Type::VARCHAR],
            )
            .await?;

        let row = c.query_opt(&stmt, &[&application, &device]).await?;

        match row {
            None => Ok(None),
            Some(row) => {
                let lost: bool = row.try_get("LOST")?;

                if !lost {
                    let created = row.try_get("CREATED")?;
                    match row.try_get::<_, Option<Json<CommandRoute>>>("DATA") {
                        Ok(Some(Json(state))) => Ok(Some(CommandRouteResponse { state, created })),
                        Ok(None) => Ok(None),
                        Err(err) => {
                            log::warn!("Failed to decode data: {err}");
                            Ok(None)
                        }
                    }
                } else {
                    // we found something, but marked as lost
                    Ok(None)
                }
            }
        }
    }
}

impl PostgresCommandRoutingService {
    pub async fn prune(&self) -> Result<(), ServiceError> {
        log::info!("Start pruning sessions");

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
    command_sessions
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
                None => {
                    log::info!("No more sessions to prune");
                    break;
                }
                Some(next) => next,
            };

            self.prune_session(t, next.try_get("ID")?).await?;
        }

        Ok(())
    }

    async fn prune_session(&self, t: Transaction<'_>, id: Uuid) -> Result<(), ServiceError> {
        log::info!("Pruning session: {id}");

        t.execute(
            r#"
DELETE FROM
    command_routes
WHERE
    SESSION = $1
"#, &[&id]).await?;

        t.execute(
            r#"
DELETE FROM
    command_sessions
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


pub async fn run_pruner(service: PostgresCommandRoutingService) -> anyhow::Result<()> {
    let period = service.timeout.to_std()?;

    loop {
        sleep(period).await;
        service.prune().await?;
    }
}
