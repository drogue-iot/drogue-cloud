use crate::error::ServiceError;
use crate::Client;
use async_trait::async_trait;
use drogue_cloud_service_api::management::{Tenant, TenantData};
use std::collections::HashSet;
use tokio_postgres::types::Json;
use tokio_postgres::Row;

#[async_trait]
pub trait TenantAccessor {
    /// Lookup a tenant
    async fn lookup(&self, alias: &str) -> Result<Option<Tenant>, ServiceError>;

    /// Delete a tenant
    async fn delete(&self, tenant_id: &str) -> Result<bool, ServiceError>;

    /// Get a tenant
    async fn get(&self, tenant_id: &str) -> Result<Option<Tenant>, ServiceError>;

    /// Create a new tenant
    async fn create(&self, tenant_id: &str, data: &TenantData) -> Result<(), ServiceError>;

    /// Update an existing tenant
    async fn update(&self, tenant_id: &str, data: &TenantData) -> Result<(), ServiceError>;
}

pub struct PostgresTenantAccessor<'c, C: Client> {
    client: &'c C,
}

impl<'c, C: Client> PostgresTenantAccessor<'c, C> {
    pub fn new(client: &'c C) -> Self {
        Self { client }
    }

    pub fn from_row(row: Row) -> Result<Tenant, tokio_postgres::Error> {
        Ok(Tenant {
            id: row.try_get::<_, String>("ID")?,
            data: row.try_get::<_, Json<_>>("DATA")?.0,
        })
    }

    async fn insert_aliases(
        &self,
        tenant_id: &str,
        aliases: &HashSet<(String, String)>,
    ) -> Result<(), tokio_postgres::Error> {
        // it doesn't make much sense to check for an empty "aliases" set, as we always
        // have the "id:<id>" alias present

        let stmt = self
            .client
            .prepare("INSERT INTO TENANT_ALIASES (ID, TYPE, ALIAS) VALUES ($1, $2, $3)")
            .await?;
        for alias in aliases {
            self.client
                .execute(&stmt, &[&tenant_id, &alias.0, &alias.1])
                .await?;
        }

        Ok(())
    }

    fn extract_aliases(tenant_id: &str, _: &TenantData) -> HashSet<(String, String)> {
        let mut aliases = HashSet::new();

        aliases.insert(("id".into(), tenant_id.into()));

        aliases
    }
}

#[async_trait]
impl<'c, C: Client> TenantAccessor for PostgresTenantAccessor<'c, C> {
    async fn lookup(&self, alias: &str) -> Result<Option<Tenant>, ServiceError> {
        let row = self.client.query_opt("SELECT T.ID, T.DATA FROM TENANT_ALIASES A INNER JOIN TENANTS T ON A.ID=T.ID WHERE A.ALIAS = $1", &[&alias]).await?;

        let result = row.map(Self::from_row).transpose()?;
        Ok(result)
    }

    async fn delete(&self, tenant_id: &str) -> Result<bool, ServiceError> {
        let count = self
            .client
            .execute("DELETE FROM TENANTS WHERE ID = $1", &[&tenant_id])
            .await?;

        Ok(count > 0)
    }

    async fn get(&self, tenant_id: &str) -> Result<Option<Tenant>, ServiceError> {
        let result = self
            .client
            .query_opt("SELECT ID, DATA FROM TENANTS WHERE ID = $1", &[&tenant_id])
            .await?
            .map(Self::from_row)
            .transpose()?;

        Ok(result)
    }

    async fn create(&self, tenant_id: &str, data: &TenantData) -> Result<(), ServiceError> {
        self.client
            .execute(
                "INSERT INTO TENANTS (ID, DATA) VALUES ($1, $2)",
                &[&tenant_id, &Json(data)],
            )
            .await?;

        let aliases = Self::extract_aliases(tenant_id, data);
        self.insert_aliases(tenant_id, &aliases).await?;

        Ok(())
    }

    async fn update(&self, tenant_id: &str, data: &TenantData) -> Result<(), ServiceError> {
        // update device
        let count = self
            .client
            .execute(
                "UPDATE TENANTS SET DATA = $2 WHERE ID = $1",
                &[&tenant_id, &Json(data)],
            )
            .await?;

        // did we update something?
        if count > 0 {
            // extract aliases
            let aliases = Self::extract_aliases(tenant_id, data);

            // clear existing aliases
            self.client
                .execute("DELETE FROM TENANT_ALIASES WHERE ID=$1", &[&tenant_id])
                .await?;

            // insert new alias set
            self.insert_aliases(tenant_id, &aliases).await?;

            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }
}
