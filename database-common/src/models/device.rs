use crate::error::ServiceError;
use crate::Client;
use async_trait::async_trait;
use drogue_cloud_service_api::management::{Credential, Device, DeviceData};
use std::collections::HashSet;
use tokio_postgres::types::Json;
use tokio_postgres::Row;

#[async_trait]
pub trait DeviceAccessor {
    /// Lookup a device by alias
    async fn lookup(&self, tenant_id: &str, alias: &str) -> Result<Option<Device>, ServiceError>;

    /// Delete a device
    async fn delete(&self, tenant_id: &str, device_id: &str) -> Result<bool, ServiceError>;

    /// Get a device
    async fn get(&self, tenant_id: &str, device_id: &str) -> Result<Option<Device>, ServiceError>;

    /// Create a new device
    async fn create(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
    ) -> Result<(), ServiceError>;

    /// Update an existing device
    async fn update(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
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
            tenant_id: row.try_get::<_, String>("TENANT_ID")?,
            id: row.try_get::<_, String>("ID")?,
            data: row.try_get::<_, Json<_>>("DATA")?.0,
        })
    }

    async fn insert_aliases(
        &self,
        tenant_id: &str,
        device_id: &str,
        aliases: &HashSet<(String, String)>,
    ) -> Result<(), tokio_postgres::Error> {
        // it doesn't make much sense to check for an empty "aliases" set, as we always
        // have the "id:<id>" alias present

        let stmt = self
            .client
            .prepare(
                "INSERT INTO DEVICE_ALIASES (TENANT_ID, ID, TYPE, ALIAS) VALUES ($1, $2, $3, $4)",
            )
            .await?;
        for alias in aliases {
            self.client
                .execute(&stmt, &[&tenant_id, &device_id, &alias.0, &alias.1])
                .await?;
        }

        Ok(())
    }

    fn extract_aliases(device_id: &str, data: &DeviceData) -> HashSet<(String, String)> {
        let mut aliases = HashSet::new();

        aliases.insert(("id".into(), device_id.into()));

        for cred in &data.credentials {
            match cred {
                Credential::UsernamePassword {
                    username, unique, ..
                } if *unique => {
                    aliases.insert(("user".into(), username.clone()));
                }
                _ => {}
            }
        }

        aliases
    }
}

#[async_trait]
impl<'c, C: Client> DeviceAccessor for PostgresDeviceAccessor<'c, C> {
    async fn lookup(&self, tenant_id: &str, alias: &str) -> Result<Option<Device>, ServiceError> {
        let result = self.client
            .query_opt("SELECT D.ID, D.TENANT_ID, D.DATA FROM DEVICE_ALIASES A INNER JOIN DEVICES D ON (A.ID=D.ID AND A.TENANT_ID=D.TENANT_ID) WHERE A.TENANT_ID = $1 AND A.ALIAS = $2", &[&tenant_id, &alias]).await?
            .map(Self::from_row).transpose()?;

        Ok(result)
    }

    async fn delete(&self, tenant_id: &str, device_id: &str) -> Result<bool, ServiceError> {
        let count = self
            .client
            .execute(
                "DELETE FROM DEVICES WHERE TENANT_ID = $1 AND ID = $2",
                &[&tenant_id, &device_id],
            )
            .await?;

        Ok(count > 0)
    }

    async fn get(&self, tenant_id: &str, device_id: &str) -> Result<Option<Device>, ServiceError> {
        let result = self
            .client
            .query_opt(
                "SELECT ID, TENANT_ID, DATA FROM DEVICES WHERE TENANT_ID = $1 AND ID = $2",
                &[&tenant_id, &device_id],
            )
            .await?
            .map(Self::from_row)
            .transpose()?;

        Ok(result)
    }

    async fn create(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
    ) -> Result<(), ServiceError> {
        self.client
            .execute(
                "INSERT INTO DEVICES (TENANT_ID, ID, DATA) VALUES ($1, $2, $3)",
                &[&tenant_id, &device_id, &Json(data)],
            )
            .await?;

        let aliases = Self::extract_aliases(device_id, data);
        self.insert_aliases(tenant_id, device_id, &aliases).await?;

        Ok(())
    }

    async fn update(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
    ) -> Result<(), ServiceError> {
        // update device
        let count = self
            .client
            .execute(
                "UPDATE DEVICES SET DATA = $3 WHERE TENANT_ID = $1 AND ID = $2",
                &[&tenant_id, &device_id, &Json(data)],
            )
            .await?;

        // did we update something?
        if count > 0 {
            // extract aliases
            let aliases = Self::extract_aliases(device_id, data);

            // clear existing aliases
            self.client
                .execute(
                    "DELETE FROM DEVICE_ALIASES WHERE TENANT_ID=$1 AND ID=$2",
                    &[&tenant_id, &device_id],
                )
                .await?;

            // insert new alias set
            self.insert_aliases(tenant_id, device_id, &aliases).await?;

            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }
}
