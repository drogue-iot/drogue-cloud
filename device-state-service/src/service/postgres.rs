use super::*;

use async_trait::async_trait;
use chrono::Utc;
use deadpool_postgres::{Pool, Transaction};
use drogue_client::registry::v1::Application;
use drogue_cloud_database_common::{postgres, Client, DatabaseService};
use drogue_cloud_endpoint_common::sender::{
    DownstreamSender, Publish, PublishError, PublishId, PublishOptions, PublishOutcome, Publisher,
};
use drogue_cloud_service_api::health::HealthChecked;
use futures::StreamExt;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use tokio_postgres::{
    types::{Json, Type},
    Row,
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
pub struct PostgresDeviceStateService {
    pool: Pool,
    sender: DownstreamSender,
    registry: Arc<dyn ApplicationLookup>,
    timeout: chrono::Duration,
}

impl PostgresDeviceStateService {
    pub fn new(
        config: PostgresServiceConfiguration,
        sender: DownstreamSender,
        registry: impl ApplicationLookup + 'static,
    ) -> anyhow::Result<Self> {
        let pool = config.pg.create_pool()?;

        let timeout = chrono::Duration::from_std(config.session_timeout)?;

        Ok(Self {
            pool,
            sender,
            registry: Arc::new(registry),
            timeout,
        })
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
            expires: now + self.timeout,
        })
    }

    async fn create(
        &self,
        session: String,
        application: String,
        device: String,
        token: String,
        state: DeviceState,
    ) -> Result<CreateResponse, ServiceError> {
        let app = match self.registry.lookup(&application).await? {
            Some(app) => app,
            None => {
                log::info!("Application not found: {application}");
                return Err(ServiceError::ApplicationNotFound);
            }
        };

        let mut c = self.pool.get().await?;

        let t = c.transaction().await?;

        let now = Utc::now();

        let r = t
            .query_opt(
                r#"
INSERT INTO
    states
(
    SESSION,
    APPLICATION,
    DEVICE,
    TOKEN,
    CREATED,
    DATA
) VALUES (
    $1::text::uuid,
    $2,
    $3,
    $4,
    $5,
    $6
)
ON CONFLICT (APPLICATION, DEVICE)
    DO UPDATE
        SET lost = true
RETURNING
    LOST
"#,
                &[&session, &application, &device, &token, &now, &Json(&state)],
            )
            .await?;

        match r {
            Some(row) => {
                let lost: bool = row.try_get("LOST")?;
                Ok(match lost {
                    false => {
                        self.send_connection_event(
                            &app,
                            PublishId {
                                name: device,
                                uid: Some(state.device_uid),
                            },
                            true,
                        )
                        .await?;
                        t.commit().await?;
                        CreateResponse::Created
                    }
                    true => {
                        t.commit().await?;
                        CreateResponse::Occupied
                    }
                })
            }
            None => Err(ServiceError::Internal("Failed to insert state".to_string())),
        }
    }

    async fn delete(
        &self,
        session: String,
        application: String,
        device: String,
        token: String,
        opts: DeleteOptions,
    ) -> Result<(), ServiceError> {
        let mut c = self.pool.get().await?;
        let t = c.transaction().await?;

        let row = t
            .query_opt(
                r#"
DELETE FROM
    states
WHERE
        SESSION = $1::text::uuid
    AND
        APPLICATION = $2
    AND
        DEVICE = $3
    AND
        TOKEN = $4
RETURNING
    APPLICATION, DEVICE, DATA
"#,
                &[&session, &application, &device, &token],
            )
            .await?;

        log::debug!("Delete result: {row:?}");

        if let Some(row) = row {
            self.send_disconnect_from_delete(row, opts).await?;
        }

        t.commit().await?;

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
    APPLICATION,
    DEVICE
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

            // convert rows to response

            let mut lost_ids = Vec::new();
            for row in r {
                let application = row.try_get("APPLICATION")?;
                let device = row.try_get("DEVICE")?;
                lost_ids.push(Id {
                    application,
                    device,
                });
            }

            // return

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
    ) -> Result<Option<DeviceStateResponse>, ServiceError> {
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
                    match row.try_get::<_, Option<Json<DeviceState>>>("DATA") {
                        Ok(Some(Json(state))) => Ok(Some(DeviceStateResponse { state, created })),
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

impl PostgresDeviceStateService {
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

        let deleted = t
            .query_raw(
                r#"
DELETE FROM
    states
WHERE
    SESSION = $1
RETURNING
    APPLICATION, DEVICE, DATA
"#,
                &[&id],
            )
            .await?;

        let mut deleted = Box::pin(deleted);

        while let Some(row) = deleted.next().await.transpose()? {
            self.send_disconnect_from_delete(row, DeleteOptions { skip_lwt: false })
                .await?;
        }

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

    /// Send a disconnected event, from a delete operation.
    ///
    /// The provided row must contain the following fields: APPLICATION, DEVICE, DATA.
    async fn send_disconnect_from_delete(
        &self,
        row: Row,
        opts: DeleteOptions,
    ) -> Result<(), ServiceError> {
        let application: String = row.try_get("APPLICATION")?;
        let device: String = row.try_get("DEVICE")?;
        let data: Option<Value> = row.try_get("DATA")?;

        log::info!("Destroying state: {application}/{device}: {data:?}");

        // we are rather conservative here, as we need to delete the record in any case
        let state = if let Some(data) = data {
            match serde_json::from_value::<DeviceState>(data) {
                Ok(data) => Some(data),
                Err(err) => {
                    log::info!("Failed to extra data for sending event: {err}");
                    None
                }
            }
        } else {
            None
        };

        let application = match self.registry.lookup(&application).await? {
            Some(app) => app,
            None => {
                log::info!("Application no longer found: {application}");
                return Ok(());
            }
        };

        let device = PublishId {
            name: device,
            uid: state.as_ref().map(|state| state.device_uid.clone()),
        };

        if !opts.skip_lwt {
            // send LWT event, if we have some
            if let Some(lwt) = state.as_ref().and_then(|s| s.lwt.as_ref()) {
                self.send_lwt_event(&application, device.clone(), lwt)
                    .await?;
            }
        }

        // send connection event

        self.send_connection_event(&application, device, false)
            .await?;

        // done

        Ok(())
    }

    async fn send_lwt_event(
        &self,
        application: &Application,
        device: PublishId,
        lwt: &LastWillTestament,
    ) -> Result<(), ServiceError> {
        self.send_event(
            Publish {
                application,
                device: device.clone(),
                sender: device,
                channel: lwt.channel.clone(),
                options: PublishOptions {
                    content_type: lwt.content_type.clone(),
                    ..Default::default()
                },
            },
            &lwt.payload,
        )
        .await
    }

    async fn send_connection_event(
        &self,
        application: &Application,
        device: PublishId,
        connected: bool,
    ) -> Result<(), ServiceError> {
        self.send_event(
            Publish {
                application,
                device: device.clone(),
                sender: device,
                channel: "connection".to_string(),
                options: PublishOptions {
                    r#type: Some(CONNECTION_TYPE_EVENT.to_string()),
                    content_type: Some("application/json".to_string()),
                    ..Default::default()
                },
            },
            serde_json::to_vec(&ConnectionEvent { connected })?,
        )
        .await
    }

    /// send a single event downstream.
    async fn send_event<B>(&self, publish: Publish<'_>, body: B) -> Result<(), ServiceError>
    where
        B: AsRef<[u8]> + Send + Sync,
    {
        let outcome = self.sender.publish(publish, body).await;

        log::debug!("Publish outcome: {outcome:?}");

        match outcome {
            Err(PublishError::Spec(err)) => {
                log::debug!("Failed to publish event due to misconfigured spec section: {err}");
                Ok(())
            }
            Err(err) => Err(ServiceError::Publish(err)),
            Ok(PublishOutcome::Accepted) => Ok(()),
            Ok(PublishOutcome::Rejected | PublishOutcome::QueueFull) => Err(
                ServiceError::Internal(format!("Unable to send event: {outcome:?}")),
            ),
        }
    }
}

pub async fn run_pruner(service: PostgresDeviceStateService) -> anyhow::Result<()> {
    let period = service.timeout.to_std()?;

    loop {
        sleep(period).await;
        service.prune().await?;
    }
}
