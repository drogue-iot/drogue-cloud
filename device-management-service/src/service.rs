use actix_web::ResponseError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use drogue_cloud_database_common::models::tenant::{PostgresTenantAccessor, TenantAccessor};
use drogue_cloud_database_common::{
    error::ServiceError,
    models::device::{DeviceAccessor, PostgresDeviceAccessor},
};
use drogue_cloud_service_api::{Device, DeviceData, Tenant, TenantData};
use serde::Deserialize;
use tokio_postgres::NoTls;

#[async_trait]
pub trait ManagementService: Clone {
    type Error: ResponseError;

    async fn is_ready(&self) -> Result<(), Self::Error>;

    async fn create_tenant(&self, tenant_id: &str, data: &TenantData) -> Result<(), Self::Error>;
    async fn update_tenant(&self, tenant_id: &str, data: &TenantData) -> Result<(), Self::Error>;
    async fn delete_tenant(&self, tenant_id: &str) -> Result<bool, Self::Error>;
    async fn get_tenant(&self, tenant_id: &str) -> Result<Option<Tenant>, Self::Error>;

    async fn create_device(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
    ) -> Result<(), Self::Error>;

    async fn update_device(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
    ) -> Result<(), Self::Error>;

    async fn delete_device(&self, tenant_id: &str, device_id: &str) -> Result<bool, Self::Error>;

    async fn get_device(
        &self,
        tenant_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct ManagementServiceConfig {
    pub pg: deadpool_postgres::Config,
}

impl ManagementServiceConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let mut cfg = config::Config::new();
        cfg.merge(config::Environment::new().separator("__"))?;
        cfg.try_into()
    }
}

#[derive(Clone)]
pub struct PostgresManagementService {
    pool: Pool,
}

impl PostgresManagementService {
    pub fn new(config: ManagementServiceConfig) -> anyhow::Result<Self> {
        Ok(Self {
            pool: config.pg.create_pool(NoTls)?,
        })
    }
}

use tokio_postgres::error::SqlState;

#[async_trait]
impl ManagementService for PostgresManagementService {
    type Error = ServiceError;

    async fn is_ready(&self) -> Result<(), Self::Error> {
        self.pool.get().await?.simple_query("SELECT 1").await?;
        Ok(())
    }

    async fn create_tenant(&self, tenant_id: &str, data: &TenantData) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresTenantAccessor::new(&t)
            .create(tenant_id, data)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            });

        t.commit().await?;

        result
    }

    async fn update_tenant(&self, tenant_id: &str, data: &TenantData) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresTenantAccessor::new(&t)
            .update(tenant_id, data)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            });

        t.commit().await?;

        result
    }

    async fn delete_tenant(&self, tenant_id: &str) -> Result<bool, Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresTenantAccessor::new(&t).delete(tenant_id).await;

        t.commit().await?;

        result
    }

    async fn get_tenant(&self, tenant_id: &str) -> Result<Option<Tenant>, Self::Error> {
        let c = self.pool.get().await?;

        let result = PostgresTenantAccessor::new(&c).get(tenant_id).await;

        result
    }

    async fn create_device(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
    ) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresDeviceAccessor::new(&t)
            .create(tenant_id, device_id, data)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                Some(state) if state == &SqlState::FOREIGN_KEY_VIOLATION => {
                    ServiceError::ReferenceNotFound
                }
                _ => err,
            });

        t.commit().await?;

        result
    }

    async fn update_device(
        &self,
        tenant_id: &str,
        device_id: &str,
        data: &DeviceData,
    ) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresDeviceAccessor::new(&t)
            .update(tenant_id, device_id, data)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            });

        t.commit().await?;

        result
    }

    async fn delete_device(&self, tenant_id: &str, device_id: &str) -> Result<bool, Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresDeviceAccessor::new(&t)
            .delete(tenant_id, device_id)
            .await;

        t.commit().await?;

        result
    }

    async fn get_device(
        &self,
        tenant_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, Self::Error> {
        let c = self.pool.get().await?;

        let result = PostgresDeviceAccessor::new(&c)
            .get(tenant_id, device_id)
            .await;

        result
    }
}
