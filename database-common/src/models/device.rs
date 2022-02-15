use crate::{
    default_resource, diffable,
    error::ServiceError,
    generation,
    models::{
        sql::{slice_iter, SelectBuilder},
        Lock, TypedAlias,
    },
    update_aliases, Client,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use drogue_client::{meta, registry};
use drogue_cloud_service_api::labels::LabelSelector;
use futures::{future, Stream, TryStreamExt};
use serde_json::Value;
use std::{
    collections::{hash_map::RandomState, HashMap, HashSet},
    pin::Pin,
};
use tokio_postgres::{
    types::{Json, ToSql, Type},
    Row,
};
use tracing::instrument;
use uuid::Uuid;

/// A device entity record.
pub struct Device {
    pub application: String,
    pub uid: Uuid,
    pub name: String,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub creation_timestamp: DateTime<Utc>,
    pub resource_version: Uuid,
    pub generation: u64,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    pub finalizers: Vec<String>,

    pub data: Value,
}

diffable!(Device);
generation!(Device => generation);
default_resource!(Device);

impl From<Device> for registry::v1::Device {
    fn from(device: Device) -> Self {
        registry::v1::Device {
            metadata: meta::v1::ScopedMetadata {
                uid: device.uid.to_string(),
                name: device.name,
                application: device.application,
                labels: device.labels,
                annotations: device.annotations,
                creation_timestamp: device.creation_timestamp,
                generation: device.generation,
                resource_version: device.resource_version.to_string(),
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
    async fn lookup(&self, app: &str, alias: &str) -> Result<Option<Device>, ServiceError>;

    /// Delete a device.
    async fn delete(&self, app: &str, device: &str) -> Result<(), ServiceError>;

    /// Get a device.
    #[instrument(name = "database_device_lookup", skip(self, lock), err)]
    async fn get(
        &self,
        app: &str,
        device: &str,
        lock: Lock,
    ) -> Result<Option<Device>, ServiceError> {
        Ok(self
            .list(
                app,
                Some(device),
                LabelSelector::default(),
                Some(1),
                None,
                lock,
            )
            .await?
            .try_next()
            .await?)
    }

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
    ) -> Result<u64, ServiceError>;

    /// Delete all devices that belong to an application.
    async fn delete_app(&self, app: &str) -> Result<u64, ServiceError>;

    /// Count devices remaining for an application.
    async fn count_devices(&self, app: &str) -> Result<u64, ServiceError>;

    /// Get a list of applications
    async fn list(
        &self,
        app: &str,
        name: Option<&str>,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
        lock: Lock,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Device, ServiceError>> + Send>>, ServiceError>;
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
            application: row.try_get("APP")?,
            uid: row.try_get("UID")?,
            name: row.try_get("NAME")?,

            creation_timestamp: row.try_get("CREATION_TIMESTAMP")?,
            generation: row.try_get::<_, i64>("GENERATION")? as u64,
            resource_version: row.try_get("RESOURCE_VERSION")?,
            labels: super::row_to_map(&row, "LABELS")?,
            annotations: super::row_to_map(&row, "ANNOTATIONS")?,
            deletion_timestamp: row.try_get("DELETION_TIMESTAMP")?,
            finalizers: super::row_to_vec(&row, "FINALIZERS")?,

            data: row.try_get::<_, Json<_>>("DATA")?.0,
        })
    }

    async fn insert_aliases(
        &self,
        app: &str,
        device: &str,
        aliases: &HashSet<TypedAlias>,
    ) -> Result<(), tokio_postgres::Error> {
        if aliases.is_empty() {
            return Ok(());
        }

        let stmt = self
            .client
            .prepare_typed(
                "INSERT INTO DEVICE_ALIASES (APP, DEVICE, TYPE, ALIAS) VALUES ($1, $2, $3, $4)",
                &[Type::VARCHAR, Type::VARCHAR, Type::VARCHAR, Type::VARCHAR],
            )
            .await?;

        for alias in aliases {
            self.client
                .execute(&stmt, &[&app, &device, &alias.0, &alias.1])
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl<'c, C: Client> DeviceAccessor for PostgresDeviceAccessor<'c, C> {
    #[instrument(name = "database_device_alias_lookup", skip(self), err)]
    async fn lookup(&self, app: &str, alias: &str) -> Result<Option<Device>, ServiceError> {
        let sql = r#"
SELECT
    D.NAME,
    D.UID,
    D.APP,
    D.LABELS,
    D.CREATION_TIMESTAMP,
    D.GENERATION,
    D.RESOURCE_VERSION,
    D.ANNOTATIONS,
    D.DELETION_TIMESTAMP,
    D.FINALIZERS,
    D.DATA
FROM
        DEVICE_ALIASES A INNER JOIN DEVICES D
    ON
        (A.DEVICE=D.NAME AND A.APP=D.APP)
WHERE
        A.APP = $1
    AND
        A.ALIAS = $2
"#;

        let stmt = self
            .client
            .prepare_typed(sql, &[Type::VARCHAR, Type::VARCHAR])
            .await?;

        let result = self
            .client
            .query_opt(&stmt, &[&app, &alias])
            .await?
            .map(Self::from_row)
            .transpose()?;

        Ok(result)
    }

    async fn delete(&self, app: &str, device: &str) -> Result<(), ServiceError> {
        let sql = "DELETE FROM DEVICES WHERE APP = $1 AND NAME = $2";

        let stmt = self
            .client
            .prepare_typed(sql, &[Type::VARCHAR, Type::VARCHAR])
            .await?;

        let count = self.client.execute(&stmt, &[&app, &device]).await?;

        if count > 0 {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }

    async fn list(
        &self,
        app: &str,
        name: Option<&str>,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
        lock: Lock,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Device, ServiceError>> + Send>>, ServiceError>
    {
        let select = r#"
SELECT
    UID,
    NAME,
    APP,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    DELETION_TIMESTAMP,
    FINALIZERS,
    DATA
FROM DEVICES
WHERE APP=$1
"#
        .to_string();

        let types: Vec<Type> = vec![Type::VARCHAR];

        let params: Vec<&(dyn ToSql + Sync)> = vec![&app];

        let builder = SelectBuilder::new(select, params, types)
            .has_where()
            .name(&name)
            .labels(&labels.0)
            .lock(lock)
            .sort(&["NAME"])
            .limit(limit)
            .offset(offset);

        let (select, params, types) = builder.build();

        let stmt = self.client.prepare_typed(&select, &types).await?;

        let stream = self
            .client
            .query_raw(&stmt, slice_iter(&params[..]))
            .await
            .map_err(|err| {
                log::debug!("Failed to get: {}", err);
                err
            })?
            .and_then(|row| future::ready(Self::from_row(row)))
            .map_err(ServiceError::Database);

        Ok(Box::pin(stream))
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
    APP,
    NAME,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
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
    $8,
    $9
)"#,
                &[
                    &device.application,
                    &device.name,
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

        self.insert_aliases(&device.application, &device.name, &aliases)
            .await?;

        Ok(())
    }

    async fn update(
        &self,
        device: Device,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<u64, ServiceError> {
        // update device
        let count = self
            .client
            .execute(
                r#"
UPDATE
    DEVICES
SET
    LABELS = $3,
    ANNOTATIONS = $4,
    GENERATION = $5,
    RESOURCE_VERSION = $6,
    DELETION_TIMESTAMP = $7,
    FINALIZERS = $8,
    DATA = $9
WHERE
    APP = $1 AND NAME = $2
"#,
                &[
                    &device.application,
                    &device.name,
                    &Json(device.labels),
                    &Json(device.annotations),
                    &(device.generation as i64),
                    &Uuid::new_v4(),
                    &device.deletion_timestamp,
                    &device.finalizers,
                    &Json(device.data),
                ],
            )
            .await
            .map_err(|err| {
                log::info!("Failed: {}", err);
                err
            })?;

        update_aliases!(count, aliases, |aliases| {
            // clear existing aliases
            let sql = "DELETE FROM DEVICE_ALIASES WHERE APP=$1 AND DEVICE=$2";
            let stmt = self
                .client
                .prepare_typed(sql, &[Type::VARCHAR, Type::VARCHAR])
                .await?;
            self.client
                .execute(&stmt, &[&device.application, &device.name])
                .await?;

            // insert new alias set
            self.insert_aliases(&device.application, &device.name, &aliases)
                .await?;

            Ok(count)
        })
    }

    async fn delete_app(&self, app_id: &str) -> Result<u64, ServiceError> {
        // delete all devices without finalizers directly

        let sql = r#"
DELETE FROM
    DEVICES
WHERE
    APP = $1
AND
    cardinality ( FINALIZERS ) = 0
"#;

        let stmt = self.client.prepare_typed(sql, &[Type::VARCHAR]).await?;
        let count = self.client.execute(&stmt, &[&app_id]).await?;

        log::debug!("Deleted {} devices without a finalizer", count);

        // count all remaining devices

        let count = self.count_devices(app_id).await?;

        log::debug!("{} devices remain for deletion", count);

        // done

        Ok(count)
    }

    async fn count_devices(&self, app_id: &str) -> Result<u64, ServiceError> {
        let sql = r#"SELECT COUNT(NAME) AS COUNT FROM DEVICES WHERE APP = $1"#;
        let stmt = self.client.prepare_typed(sql, &[Type::VARCHAR]).await?;
        let count = self
            .client
            .query_opt(&stmt, &[&app_id])
            .await?
            .ok_or_else(|| {
                ServiceError::Internal("Unable to retrieve number of devices with finalizer".into())
            })?;

        Ok(count.try_get::<_, i64>("COUNT")? as u64)
    }
}
