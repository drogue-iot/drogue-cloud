use crate::service::ServiceError;
use async_trait::async_trait;
use deadpool_postgres::ClientWrapper;
use drogue_cloud_service_api::Tenant;
use tokio_postgres::types::Json;
use tokio_postgres::Row;

#[async_trait]
pub trait TenantAccessor {
    async fn lookup(&self, alias: &str) -> Result<Option<Tenant>, ServiceError>;
}

pub struct PostgresTenantAccessor<'c> {
    client: &'c ClientWrapper,
}

impl<'c> PostgresTenantAccessor<'c> {
    pub fn new(client: &'c ClientWrapper) -> Self {
        Self { client }
    }

    pub fn from_row(row: Row) -> Result<Tenant, tokio_postgres::Error> {
        Ok(Tenant {
            id: row.try_get::<_, String>("ID")?,
            data: row.try_get::<_, Json<_>>("DATA")?.0,
        })
    }
}

impl<'c> From<&'c ClientWrapper> for PostgresTenantAccessor<'c> {
    fn from(client: &'c ClientWrapper) -> Self {
        PostgresTenantAccessor::new(client)
    }
}

#[async_trait]
impl<'c> TenantAccessor for PostgresTenantAccessor<'c> {
    async fn lookup(&self, alias: &str) -> Result<Option<Tenant>, ServiceError> {
        let stmt = self
            .client
            .prepare(
                "SELECT T.ID, T.DATA FROM TENANT_ALIASES A INNER JOIN TENANTS T ON A.ID=T.ID WHERE A.ALIAS = $1",
            )
            .await?;

        log::debug!("Prepared statement");

        let row = self.client.query_opt(&stmt, &[&alias]).await?;

        log::debug!("Found {}", row.is_some());

        let result = row.map(Self::from_row).transpose()?;
        Ok(result)
    }
}
