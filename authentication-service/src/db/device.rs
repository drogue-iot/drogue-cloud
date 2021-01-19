use crate::service::ServiceError;
use async_trait::async_trait;
use deadpool_postgres::ClientWrapper;
use drogue_cloud_service_api::Device;
use tokio_postgres::types::Json;
use tokio_postgres::Row;

#[async_trait]
pub trait DeviceAccessor {
    async fn lookup(&self, tenant_id: &str, alias: &str) -> Result<Option<Device>, ServiceError>;
}

pub struct PostgresDeviceAccessor<'c> {
    client: &'c ClientWrapper,
}

impl<'c> PostgresDeviceAccessor<'c> {
    pub fn new(client: &'c ClientWrapper) -> Self {
        Self { client }
    }

    pub fn from_row(row: Row) -> Result<Device, tokio_postgres::Error> {
        Ok(Device {
            tenant_id: row.try_get::<_, String>("TENANT_ID")?,
            id: row.try_get::<_, String>("ID")?,
            data: row.try_get::<_, Json<_>>("DATA")?.0,
        })
    }
}

impl<'c> From<&'c ClientWrapper> for PostgresDeviceAccessor<'c> {
    fn from(client: &'c ClientWrapper) -> Self {
        PostgresDeviceAccessor::new(client)
    }
}

#[async_trait]
impl<'c> DeviceAccessor for PostgresDeviceAccessor<'c> {
    async fn lookup(&self, tenant_id: &str, alias: &str) -> Result<Option<Device>, ServiceError> {
        let stmt = self
            .client
            .prepare("SELECT D.ID, D.TENANT_ID, D.DATA FROM DEVICE_ALIASES A INNER JOIN DEVICES D ON (A.ID=D.ID AND A.TENANT_ID=D.TENANT_ID) WHERE A.TENANT_ID = $1 AND A.ALIAS = $2")
            .await?;

        log::debug!("Prepared statement");

        let rows = self.client.query_opt(&stmt, &[&tenant_id, &alias]).await?;

        log::debug!("Found {}", rows.is_some());

        let result = rows.map(Self::from_row).transpose()?;

        Ok(result)
    }
}
