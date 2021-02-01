mod x509;

use actix_web::ResponseError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use drogue_cloud_database_common::{
    error::ServiceError,
    models::{
        self,
        app::{ApplicationAccessor, PostgresApplicationAccessor},
        device::{DeviceAccessor, PostgresDeviceAccessor},
        TypedAlias,
    },
};
use drogue_cloud_service_api::{
    management::{
        Application, ApplicationSpecTrustAnchors, Credential, Device, DeviceSpecCredentials,
    },
    Translator,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use tokio_postgres::{error::SqlState, NoTls};

#[async_trait]
pub trait ManagementService: Clone {
    type Error: ResponseError;

    async fn is_ready(&self) -> Result<(), Self::Error>;

    async fn create_app(&self, data: Application) -> Result<(), Self::Error>;
    async fn get_app(&self, id: &str) -> Result<Option<Application>, Self::Error>;
    async fn update_app(&self, data: Application) -> Result<(), Self::Error>;
    async fn delete_app(&self, id: &str) -> Result<bool, Self::Error>;

    async fn create_device(&self, device: Device) -> Result<(), Self::Error>;
    async fn get_device(
        &self,
        app_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, Self::Error>;
    async fn update_device(&self, device: Device) -> Result<(), Self::Error>;
    async fn delete_device(&self, app_id: &str, device_id: &str) -> Result<bool, Self::Error>;
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

    fn app_to_entity(
        mut app: Application,
    ) -> Result<(models::app::Application, HashSet<TypedAlias>), ServiceError> {
        // extract aliases

        let mut aliases = HashSet::with_capacity(1);
        aliases.insert(TypedAlias("id".into(), app.metadata.name.clone()));

        // extract trust anchors

        match app.spec_as::<ApplicationSpecTrustAnchors, _>("trustAnchors") {
            Some(Ok(anchors)) => {
                log::debug!("Anchors: {:?}", anchors);
                let status = x509::process_anchors(anchors)?;

                // add aliases
                aliases.extend(status.1);

                // inject status section
                app.status.insert(
                    "trustAnchors".into(),
                    serde_json::to_value(status.0)
                        .map_err(|err| ServiceError::BadRequest(err.to_string()))?,
                );
            }
            r => log::debug!("No-anchors: {:?}", r),
        }

        // convert payload

        let app = models::app::Application {
            id: app.metadata.name,
            labels: app.metadata.labels,
            data: json!({
                "spec": app.spec,
                "status": app.status,
            }),
        };

        // return result

        Ok((app, aliases))
    }

    fn device_to_entity(
        device: Device,
    ) -> Result<(models::device::Device, HashSet<TypedAlias>), ServiceError> {
        // extract aliases

        let mut aliases = HashSet::new();

        aliases.insert(TypedAlias("id".into(), device.metadata.name.clone()));

        if let Some(Ok(credentials)) = device.spec_as::<DeviceSpecCredentials, _>("credentials") {
            for credential in credentials.credentials {
                match credential {
                    Credential::UsernamePassword {
                        username, unique, ..
                    } if unique => {
                        aliases.insert(TypedAlias("username".into(), username));
                    }
                    _ => {}
                }
            }
        }

        // convert payload

        let device = models::device::Device {
            id: device.metadata.name,
            application_id: device.metadata.application,
            labels: device.metadata.labels,
            data: json!({
                "spec": device.spec,
                "status": device.status,
            }),
        };

        // return result

        Ok((device, aliases))
    }
}

#[async_trait]
impl ManagementService for PostgresManagementService {
    type Error = ServiceError;

    async fn is_ready(&self) -> Result<(), Self::Error> {
        self.pool.get().await?.simple_query("SELECT 1").await?;
        Ok(())
    }

    async fn create_app(&self, application: Application) -> Result<(), Self::Error> {
        let (app, aliases) = Self::app_to_entity(application)?;

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresApplicationAccessor::new(&t)
            .create(app, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            });

        t.commit().await?;

        result
    }

    async fn get_app(&self, id: &str) -> Result<Option<Application>, Self::Error> {
        let c = self.pool.get().await?;

        let app = PostgresApplicationAccessor::new(&c).get(id).await?;

        Ok(app.map(Into::into))
    }

    async fn update_app(&self, application: Application) -> Result<(), Self::Error> {
        let (app, aliases) = Self::app_to_entity(application)?;

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresApplicationAccessor::new(&t)
            .update(app, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            });

        t.commit().await?;

        result
    }

    async fn delete_app(&self, id: &str) -> Result<bool, Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresApplicationAccessor::new(&t).delete(id).await;

        t.commit().await?;

        result
    }

    async fn create_device(&self, device: Device) -> Result<(), Self::Error> {
        let (device, aliases) = Self::device_to_entity(device)?;

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresDeviceAccessor::new(&t)
            .create(device, aliases)
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

    async fn get_device(
        &self,
        app_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, Self::Error> {
        let c = self.pool.get().await?;

        let device = PostgresDeviceAccessor::new(&c)
            .get(app_id, device_id)
            .await?;

        Ok(device.map(Into::into))
    }

    async fn update_device(&self, device: Device) -> Result<(), Self::Error> {
        let (device, aliases) = Self::device_to_entity(device)?;

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresDeviceAccessor::new(&t)
            .update(device, aliases)
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
}
