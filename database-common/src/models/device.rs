use crate::{error::ServiceError, models::TypedAlias, Client};
use async_trait::async_trait;
use drogue_cloud_service_api::management::{self, DeviceMetadata};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tokio_postgres::{types::Json, Row};

/// A device entity record.
pub struct Device {
    pub application_id: String,
    pub id: String,
    pub labels: HashMap<String, String>,
    pub data: Value,
}

impl From<Device> for management::Device {
    fn from(device: Device) -> Self {
        management::Device {
            metadata: DeviceMetadata {
                name: device.id,
                application: device.application_id,
                labels: device.labels,
                ..Default::default()
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
    /// Lookup a device by alias
    async fn lookup(&self, app_id: &str, alias: &str) -> Result<Option<Device>, ServiceError>;

    /// Delete a device
    async fn delete(&self, app_id: &str, device_id: &str) -> Result<bool, ServiceError>;

    /// Get a device
    async fn get(&self, app_id: &str, device_id: &str) -> Result<Option<Device>, ServiceError>;

    /// Create a new device
    async fn create(
        &self,
        device: Device,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError>;

    /// Update an existing device
    async fn update(
        &self,
        device: Device,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError>;
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
            application_id: row.try_get::<_, String>("APP_ID")?,
            id: row.try_get::<_, String>("ID")?,
            data: row.try_get::<_, Json<_>>("DATA")?.0,
            labels: super::labels_to_map(&row)?,
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
        let result = self.client
            .query_opt("SELECT D.ID, D.APP_ID, D.LABELS, D.DATA FROM DEVICE_ALIASES A INNER JOIN DEVICES D ON (A.ID=D.ID AND A.APP_ID=D.APP_ID) WHERE A.APP_ID = $1 AND A.ALIAS = $2", &[&app_id, &alias]).await?
            .map(Self::from_row).transpose()?;

        Ok(result)
    }

    async fn delete(&self, app_id: &str, device_id: &str) -> Result<bool, ServiceError> {
        let count = self
            .client
            .execute(
                "DELETE FROM DEVICES WHERE APP_ID = $1 AND ID = $2",
                &[&app_id, &device_id],
            )
            .await?;

        Ok(count > 0)
    }

    async fn get(&self, app_id: &str, device_id: &str) -> Result<Option<Device>, ServiceError> {
        let result = self
            .client
            .query_opt(
                "SELECT ID, APP_ID, LABELS, DATA FROM DEVICES WHERE APP_ID = $1 AND ID = $2",
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
                "INSERT INTO DEVICES (APP_ID, ID, LABELS, DATA) VALUES ($1, $2, $3, $4)",
                &[
                    &device.application_id,
                    &device.id,
                    &Json(&device.labels),
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
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError> {
        // update device
        let count = self
            .client
            .execute(
                "UPDATE DEVICES SET LABELS = $3, DATA = $4 WHERE APP_ID = $1 AND ID = $2",
                &[
                    &device.application_id,
                    &device.id,
                    &Json(device.labels),
                    &Json(device.data),
                ],
            )
            .await?;

        // did we update something?
        if count > 0 {
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
        } else {
            Err(ServiceError::NotFound)
        }
    }
}
