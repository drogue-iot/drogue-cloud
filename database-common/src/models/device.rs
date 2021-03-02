use crate::{
    diffable,
    error::ServiceError,
    generation,
    models::{Lock, TypedAlias},
    update_aliases, Client,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use drogue_cloud_service_api::management::{self, ScopedMetadata};
use serde_json::Value;
use std::collections::{hash_map::RandomState, HashMap, HashSet};
use tokio_postgres::{types::Json, Row};
use uuid::Uuid;

/// A device entity record.
pub struct Device {
    pub application_id: String,
    pub id: String,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub creation_timestamp: DateTime<Utc>,
    pub resource_version: String,
    pub generation: u64,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    pub finalizers: Vec<String>,

    pub data: Value,
}

diffable!(Device);
generation!(Device => generation);

impl From<Device> for management::Device {
    fn from(device: Device) -> Self {
        management::Device {
            metadata: ScopedMetadata {
                name: device.id,
                application: device.application_id,
                labels: device.labels,
                annotations: device.annotations,
                creation_timestamp: device.creation_timestamp,
                generation: device.generation,
                resource_version: device.resource_version,
                deletion_timestamp: device.deletion_timestamp,
                finalizers: device.finalizers,
            },
            spec: device.data["spec"].as_object().cloned().unwrap_or_default(),
            status: device.data["status"]
                .as_object()
                .cloned()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
pub trait DeviceAccessor {
    /// Lookup a device by alias.
    async fn lookup(&self, app_id: &str, alias: &str) -> Result<Option<Device>, ServiceError>;

    /// Delete a device.
    async fn delete(&self, app_id: &str, device_id: &str) -> Result<(), ServiceError>;

    /// Get a device.
    async fn get(
        &self,
        app_id: &str,
        device_id: &str,
        lock: Lock,
    ) -> Result<Option<Device>, ServiceError>;

    /// Create a new device.
    async fn create(
        &self,
        device: Device,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError>;

    /// Update an existing device.
    async fn update(
        &self,
        device: Device,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<(), ServiceError>;

    /// Delete all devices that belong to an application.
    async fn delete_app(&self, app_id: &str) -> Result<u64, ServiceError>;

    /// Count devices remaining for an application.
    async fn count_devices(&self, app_id: &str) -> Result<u64, ServiceError>;
}

pub struct PostgresDeviceAccessor<'c, C: Client> {
    client: &'c C,
}

impl<'c, C: Client> PostgresDeviceAccessor<'c, C> {
    pub fn new(client: &'c C) -> Self {
        Self { client }
    }

    pub fn from_row(row: Row) -> Result<Device, tokio_postgres::Error> {
        Ok(Device {
            application_id: row.try_get("APP_ID")?,
            id: row.try_get("ID")?,

            creation_timestamp: row.try_get("CREATION_TIMESTAMP")?,
            generation: row.try_get::<_, i64>("GENERATION")? as u64,
            resource_version: row.try_get::<_, Uuid>("RESOURCE_VERSION")?.to_string(),
            labels: super::row_to_map(&row, "LABELS")?,
            annotations: super::row_to_map(&row, "ANNOTATIONS")?,
            deletion_timestamp: row.try_get("DELETION_TIMESTAMP")?,
            finalizers: super::row_to_vec(&row, "FINALIZERS")?,

            data: row.try_get::<_, Json<_>>("DATA")?.0,
        })
    }

    async fn insert_aliases(
        &self,
        app_id: &str,
        device_id: &str,
        aliases: &HashSet<TypedAlias>,
    ) -> Result<(), tokio_postgres::Error> {
        if aliases.is_empty() {
            return Ok(());
        }

        let stmt = self
            .client
            .prepare("INSERT INTO DEVICE_ALIASES (APP_ID, ID, TYPE, ALIAS) VALUES ($1, $2, $3, $4)")
            .await?;

        for alias in aliases {
            self.client
                .execute(&stmt, &[&app_id, &device_id, &alias.0, &alias.1])
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl<'c, C: Client> DeviceAccessor for PostgresDeviceAccessor<'c, C> {
    async fn lookup(&self, app_id: &str, alias: &str) -> Result<Option<Device>, ServiceError> {
        let result = self
            .client
            .query_opt(
                r#"
SELECT
    D.ID, D.APP_ID, D.LABELS, D.CREATION_TIMESTAMP, D.GENERATION, D.RESOURCE_VERSION, D.ANNOTATIONS, D.DELETION_TIMESTAMP, D.FINALIZERS, D.DATA
FROM
    DEVICE_ALIASES A INNER JOIN DEVICES D ON (A.ID=D.ID AND A.APP_ID=D.APP_ID)
WHERE
    A.APP_ID = $1
    AND
    A.ALIAS = $2"#,
                &[&app_id, &alias],
            )
            .await?
            .map(Self::from_row)
            .transpose()?;

        Ok(result)
    }

    async fn delete(&self, app_id: &str, device_id: &str) -> Result<(), ServiceError> {
        let count = self
            .client
            .execute(
                "DELETE FROM DEVICES WHERE APP_ID = $1 AND ID = $2",
                &[&app_id, &device_id],
            )
            .await?;

        if count > 0 {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }

    async fn get(
        &self,
        app_id: &str,
        device_id: &str,
        lock: Lock,
    ) -> Result<Option<Device>, ServiceError> {
        let result = self
            .client
            .query_opt(
                format!(
                    r#"
SELECT
    ID,
    APP_ID,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    DELETION_TIMESTAMP,
    FINALIZERS,
    DATA
FROM DEVICES
WHERE
    APP_ID = $1 AND ID = $2
{for_update}
"#,
                    for_update = lock.to_string()
                )
                .as_str(),
                &[&app_id, &device_id],
            )
            .await?
            .map(Self::from_row)
            .transpose()?;

        Ok(result)
    }

    async fn create(
        &self,
        device: Device,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError> {
        self.client
            .execute(
                r#"
INSERT INTO DEVICES (
    APP_ID,
    ID,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    DELETION_TIMESTAMP,
    FINALIZERS,
    DATA
) VALUES (
    $1,
    $2,
    $3,
    $4,
    $5,
    $6,
    $7,
    NULL,
    $8,
    $9
)"#,
                &[
                    &device.application_id,
                    &device.id,
                    &Json(&device.labels),
                    &Json(&device.annotations),
                    &Utc::now(),
                    &(device.generation as i64),
                    &Uuid::new_v4(),
                    &device.finalizers,
                    &Json(&device.data),
                ],
            )
            .await?;

        self.insert_aliases(&device.application_id, &device.id, &aliases)
            .await?;

        Ok(())
    }

    async fn update(
        &self,
        device: Device,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<(), ServiceError> {
        // update device
        let count = self
            .client
            .execute(
                r#"
UPDATE DEVICES SET
    LABELS = $3,
    ANNOTATIONS = $4,
    GENERATION = $5,
    RESOURCE_VERSION = $6,
    DELETION_TIMESTAMP = $7,
    FINALIZERS = $8,
    DATA = $9
WHERE
    APP_ID = $1 AND ID = $2
"#,
                &[
                    &device.application_id,
                    &device.id,
                    &Json(device.labels),
                    &Json(device.annotations),
                    &(device.generation as i64),
                    &Uuid::new_v4(),
                    &device.deletion_timestamp,
                    &device.finalizers,
                    &Json(device.data),
                ],
            )
            .await?;

        update_aliases!(count, aliases, |aliases| {
            // clear existing aliases
            self.client
                .execute(
                    "DELETE FROM DEVICE_ALIASES WHERE APP_ID=$1 AND ID=$2",
                    &[&device.application_id, &device.id],
                )
                .await?;

            // insert new alias set
            self.insert_aliases(&device.application_id, &device.id, &aliases)
                .await?;

            Ok(())
        })
    }

    async fn delete_app(&self, app_id: &str) -> Result<u64, ServiceError> {
        // delete all devices without finalizers directly

        let count = self
            .client
            .execute(
                r#"
DELETE
    FROM DEVICES
WHERE
    APP_ID = $1
AND
    cardinality ( FINALIZERS ) = 0
"#,
                &[&app_id],
            )
            .await?;

        log::debug!("Deleted {} devices without a finalizer", count);

        // count all remaining devices

        let count = self.count_devices(&app_id).await?;

        log::debug!("{} devices remain for deletion", count);

        // done

        Ok(count)
    }

    async fn count_devices(&self, app_id: &str) -> Result<u64, ServiceError> {
        let count = self
            .client
            .query_opt(
                r#"SELECT COUNT(ID) AS COUNT FROM DEVICES WHERE APP_ID = $1"#,
                &[&app_id],
            )
            .await?
            .ok_or_else(|| {
                ServiceError::Internal("Unable to retrieve number of devices with finalizer".into())
            })?;

        Ok(count.try_get::<_, i64>("COUNT")? as u64)
    }
}
