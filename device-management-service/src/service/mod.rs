mod error;
mod x509;

use crate::{service::error::PostgresManagementServiceError, utils::epoch};
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
    DatabaseService,
};
use drogue_cloud_registry_events::EventSender;
use drogue_cloud_service_api::{
    health::HealthCheckedService,
    management::{
        Application, ApplicationSpecTrustAnchors, Credential, Device, DeviceSpecCredentials,
    },
    Translator,
};
use drogue_cloud_service_common::config::ConfigFromEnv;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use tokio_postgres::{error::SqlState, NoTls};

#[async_trait]
pub trait ManagementService: HealthCheckedService + Clone {
    type Error: ResponseError;

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
pub struct PostgresManagementServiceConfig {
    pub pg: deadpool_postgres::Config,
    pub instance: String,
}

impl<'de> ConfigFromEnv<'de> for PostgresManagementServiceConfig {}

impl<S> DatabaseService for PostgresManagementService<S>
where
    S: EventSender,
{
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait]
impl<S> HealthCheckedService for PostgresManagementService<S>
where
    S: EventSender,
{
    type HealthCheckError = ServiceError;

    async fn is_ready(&self) -> Result<(), Self::HealthCheckError> {
        (self as &dyn DatabaseService).is_ready().await
    }
}

#[derive(Clone)]
pub struct PostgresManagementService<S>
where
    S: EventSender,
{
    pool: Pool,
    sender: S,
    instance: String,
}

impl<S> PostgresManagementService<S>
where
    S: EventSender,
{
    pub fn new(config: PostgresManagementServiceConfig, sender: S) -> anyhow::Result<Self> {
        Ok(Self {
            pool: config.pg.create_pool(NoTls)?,
            instance: config.instance,
            sender,
        })
    }

    fn app_to_entity(
        mut app: Application,
    ) -> Result<
        (models::app::Application, HashSet<TypedAlias>),
        PostgresManagementServiceError<S::Error>,
    > {
        // extract aliases

        let mut aliases = HashSet::with_capacity(1);
        aliases.insert(TypedAlias("id".into(), app.metadata.name.clone()));

        // extract trust anchors

        match app.section::<ApplicationSpecTrustAnchors>() {
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
            annotations: app.metadata.annotations,
            generation: 0,                   // will be set internally
            creation_timestamp: epoch(),     // will be set internally
            resource_version: String::new(), // will be set internally
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
    ) -> Result<
        (models::device::Device, HashSet<TypedAlias>),
        PostgresManagementServiceError<S::Error>,
    > {
        // extract aliases

        let mut aliases = HashSet::new();

        aliases.insert(TypedAlias("id".into(), device.metadata.name.clone()));

        if let Some(Ok(credentials)) = device.section::<DeviceSpecCredentials>() {
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
            annotations: device.metadata.annotations,
            creation_timestamp: epoch(),     // will be set internally
            generation: 0,                   // will be set internally
            resource_version: String::new(), // will be set internally
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
impl<S> ManagementService for PostgresManagementService<S>
where
    S: EventSender,
{
    type Error = PostgresManagementServiceError<S::Error>;

    async fn create_app(&self, application: Application) -> Result<(), Self::Error> {
        let (app, aliases) = Self::app_to_entity(application)?;

        let id = app.id.clone();

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        PostgresApplicationAccessor::new(&t)
            .create(app, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            })?;

        // commit

        t.commit().await?;

        // send change events

        self.sender
            .notify_app(self.instance.clone(), id, &[])
            .await?;

        // done

        Ok(())
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

        PostgresApplicationAccessor::new(&t)
            .update(app, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            })?;

        t.commit().await?;

        Ok(())
    }

    async fn delete_app(&self, id: &str) -> Result<bool, Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresApplicationAccessor::new(&t).delete(id).await?;

        // commit

        t.commit().await?;

        // send change event

        self.sender
            .notify_app(self.instance.clone(), id, &[])
            .await?;

        // done

        Ok(result)
    }

    async fn create_device(&self, device: Device) -> Result<(), Self::Error> {
        let (device, aliases) = Self::device_to_entity(device)?;

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        PostgresDeviceAccessor::new(&t)
            .create(device, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                Some(state) if state == &SqlState::FOREIGN_KEY_VIOLATION => {
                    ServiceError::ReferenceNotFound
                }
                _ => err,
            })?;

        t.commit().await?;

        // send change events

        self.sender
            .notify_device(self.instance.clone(), app, id, &[])
            .await?;

        Ok(())
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

        PostgresDeviceAccessor::new(&t)
            .update(device, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                _ => err,
            })?;

        t.commit().await?;

        Ok(())
    }

    async fn delete_device(&self, tenant_id: &str, device_id: &str) -> Result<bool, Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let result = PostgresDeviceAccessor::new(&t)
            .delete(tenant_id, device_id)
            .await?;

        t.commit().await?;

        Ok(result)
    }
}
