mod error;
mod x509;

use crate::{service::error::PostgresManagementServiceError, utils::epoch};
use actix_web::ResponseError;
use async_trait::async_trait;
use chrono::Utc;
use deadpool_postgres::{Pool, Transaction};
use drogue_cloud_database_common::models::outbox::PostgresOutboxAccessor;
use drogue_cloud_database_common::models::Generation;
use drogue_cloud_database_common::{
    error::ServiceError,
    models::{
        self,
        app::{ApplicationAccessor, PostgresApplicationAccessor},
        device::{DeviceAccessor, PostgresDeviceAccessor},
        diff::diff_paths,
        Lock, TypedAlias,
    },
    DatabaseService,
};
use drogue_cloud_registry_events::{Event, EventSender, EventSenderError, SendEvent};
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
    async fn delete_app(&self, id: &str) -> Result<(), Self::Error>;

    async fn create_device(&self, device: Device) -> Result<(), Self::Error>;
    async fn get_device(
        &self,
        app_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, Self::Error>;
    async fn update_device(&self, device: Device) -> Result<(), Self::Error>;
    async fn delete_device(&self, app_id: &str, device_id: &str) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct PostgresManagementServiceConfig {
    pub pg: deadpool_postgres::Config,
    pub instance: String,
}

impl<'de> ConfigFromEnv<'de> for PostgresManagementServiceConfig {}

impl<S> DatabaseService for PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait]
impl<S> HealthCheckedService for PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    type HealthCheckError = ServiceError;

    async fn is_ready(&self) -> Result<(), Self::HealthCheckError> {
        (self as &dyn DatabaseService).is_ready().await
    }
}

#[derive(Clone)]
pub struct PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    pool: Pool,
    sender: S,
    instance: String,
}

impl<S> PostgresManagementService<S>
where
    S: EventSender + Clone,
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
            deletion_timestamp: None,        // will be set internally
            finalizers: app.metadata.finalizers,
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
            deletion_timestamp: None,        // will be set internally
            finalizers: device.metadata.finalizers,
            data: json!({
                "spec": device.spec,
                "status": device.status,
            }),
        };

        // return result

        Ok((device, aliases))
    }

    /// Perform the operation of updating an application
    async fn perform_update_app(
        &self,
        t: &Transaction<'_>,
        mut app: models::app::Application,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<Vec<Event>, PostgresManagementServiceError<S::Error>> {
        let id = app.id.clone();

        let accessor = PostgresApplicationAccessor::new(t);

        // get current state for diffing
        let current = match accessor.get(&app.id, Lock::ForUpdate).await? {
            Some(app) => Ok(app),
            None => Err(ServiceError::NotFound),
        }?;

        // we simply copy over the deletion timestamp
        app.deletion_timestamp = current.deletion_timestamp;

        if app.deletion_timestamp.is_some() && app.finalizers.is_empty() {
            // delete, but don't send any event
            accessor.delete(&id).await?;

            Ok(vec![])
        } else {
            // check which paths changed

            let paths = diff_paths(&current, &app);
            if paths.is_empty() {
                // there was no change
                return Ok(vec![]);
            }

            // next generation
            let generation = app.next_generation()?;

            // update

            accessor
                .update(app, aliases)
                .await
                .map_err(|err| match err.sql_state() {
                    Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                    _ => err,
                })?;

            // send change event

            Ok(Event::new_app(self.instance.clone(), id, generation, paths))
        }
    }

    /// Called when a device was deleted, so check if the application can be garbage collected.
    async fn check_clean_app(
        &self,
        t: &Transaction<'_>,
        app_id: &str,
    ) -> Result<(), PostgresManagementServiceError<S::Error>> {
        let app = PostgresApplicationAccessor::new(t)
            .get(app_id, Lock::ForUpdate)
            .await?;

        let mut app = if let Some(app) = app {
            app
        } else {
            // device without an app, shouldn't happen, but don't need to do anything anyways.
            return Ok(());
        };

        if app.deletion_timestamp.is_none() {
            // device got deleted, but the app is not
            return Ok(());
        }

        // check how many devices remain

        let count = PostgresDeviceAccessor::new(t).count_devices(app_id).await?;
        if count > 0 {
            // there are still devices left.
            return Ok(());
        }

        // we removed the last of the devices blocking the deletion
        app.finalizers.retain(|f| f != "has-devices");
        self.perform_update_app(t, app, None).await?;

        // done

        Ok(())
    }

    fn outbox_err<E>(err: EventSenderError<ServiceError>) -> PostgresManagementServiceError<E>
    where
        E: std::error::Error + std::fmt::Debug + 'static,
    {
        match err {
            EventSenderError::Sender(err) => PostgresManagementServiceError::Service(err),
            EventSenderError::CloudEvent(err) => {
                PostgresManagementServiceError::EventSender(EventSenderError::CloudEvent(err))
            }
            EventSenderError::Event(err) => {
                PostgresManagementServiceError::EventSender(EventSenderError::Event(err))
            }
        }
    }
}

#[async_trait]
impl<S> ManagementService for PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    type Error = PostgresManagementServiceError<S::Error>;

    async fn create_app(&self, application: Application) -> Result<(), Self::Error> {
        let (mut app, aliases) = Self::app_to_entity(application)?;

        let generation = app.next_generation()?;

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

        let events = Event::new_app(self.instance.clone(), id, generation, vec![]);
        events
            .clone()
            .send_with(&PostgresOutboxAccessor::new(&t))
            .await
            .map_err(Self::outbox_err)?;

        // commit

        t.commit().await?;

        // send change events

        events.send_with(&self.sender).await?;

        // done

        Ok(())
    }

    async fn get_app(&self, id: &str) -> Result<Option<Application>, Self::Error> {
        let c = self.pool.get().await?;

        let app = PostgresApplicationAccessor::new(&c)
            .get(id, Lock::None)
            .await?;

        Ok(app.map(Into::into))
    }

    async fn update_app(&self, application: Application) -> Result<(), Self::Error> {
        let (app, aliases) = Self::app_to_entity(application)?;

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let events = self.perform_update_app(&t, app, Some(aliases)).await?;

        t.commit().await?;

        // send events

        events.send_with(&self.sender).await?;

        Ok(())
    }

    async fn delete_app(&self, id: &str) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // get current state for diffing
        let mut current = match accessor.get(&id, Lock::ForUpdate).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        if current.deletion_timestamp.is_some() {
            return Err(ServiceError::NotFound.into());
        }

        // next, we need to delete the application

        // first, delete all devices and count the once we can only soft-delete

        let remaining_devices = PostgresDeviceAccessor::new(&t).delete_app(&id).await?;

        if remaining_devices > 0 {
            // we have pending device deletions, so add the finalizer
            current.finalizers.push("has-devices".into());
        }

        // next generation
        let generation = current.next_generation()?;

        // then delete the application

        let path = if current.finalizers.is_empty() {
            accessor.delete(id).await?;

            "."
        } else {
            // update deleted timestamp
            current.deletion_timestamp = Some(Utc::now());

            // update the record
            accessor.update(current, None).await?;

            ".metadata"
        }
        .into();

        // commit

        t.commit().await?;

        // send change event

        Event::new_app(self.instance.clone(), id, generation, vec![path])
            .send_with(&self.sender)
            .await?;

        // done

        Ok(())
    }

    async fn create_device(&self, device: Device) -> Result<(), Self::Error> {
        let (mut device, aliases) = Self::device_to_entity(device)?;

        let generation = device.next_generation()?;

        let app_id = device.application_id.clone();
        let id = device.id.clone();

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let app = PostgresApplicationAccessor::new(&t)
            .get(&app_id, Lock::ForShare)
            .await?;

        // if there is no entry, or it is marked for deletion, we don't allow adding a new device

        match app {
            Some(app) if app.deletion_timestamp.is_none() => {}
            _ => return Err(ServiceError::ReferenceNotFound.into()),
        }

        // create the device

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

        // create and persist events

        let events = Event::new_device(self.instance.clone(), app_id, id, generation, vec![]);
        events
            .clone()
            .send_with(&PostgresOutboxAccessor::new(&t))
            .await
            .map_err(Self::outbox_err)?;

        t.commit().await?;

        // send change events

        events.send_with(&self.sender).await?;

        // done

        Ok(())
    }

    async fn get_device(
        &self,
        app_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, Self::Error> {
        let c = self.pool.get().await?;

        let device = PostgresDeviceAccessor::new(&c)
            .get(app_id, device_id, Lock::None)
            .await?;

        Ok(device.map(Into::into))
    }

    async fn update_device(&self, device: Device) -> Result<(), Self::Error> {
        let (mut device, aliases) = Self::device_to_entity(device)?;

        let app_id = device.application_id.clone();
        let id = device.id.clone();

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresDeviceAccessor::new(&t);

        // get current state for diffing
        let current = match accessor.get(&app_id, &id, Lock::ForUpdate).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        // we simply copy over the deletion timestamp
        device.deletion_timestamp = current.deletion_timestamp;

        if device.deletion_timestamp.is_some() && device.finalizers.is_empty() {
            // delete, but don't send any event
            accessor.delete(&app_id, &id).await?;

            // check with the application
            self.check_clean_app(&t, &app_id).await?;

            t.commit().await?;
        } else {
            // check which paths changed
            let paths = diff_paths(&current, &device);
            if paths.is_empty() {
                // there was no change
                return Ok(());
            }

            let generation = device.next_generation()?;

            accessor
                .update(device, Some(aliases))
                .await
                .map_err(|err| match err.sql_state() {
                    Some(state) if state == &SqlState::UNIQUE_VIOLATION => ServiceError::Conflict,
                    _ => err,
                })?;

            t.commit().await?;

            // send change event

            Event::new_device(self.instance.clone(), app_id, id, generation, paths)
                .send_with(&self.sender)
                .await?;
        }

        // done

        Ok(())
    }

    async fn delete_device(&self, app_id: &str, device_id: &str) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresDeviceAccessor::new(&t);

        // get current state for diffing
        let mut current = match accessor.get(&app_id, &device_id, Lock::ForUpdate).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        if current.deletion_timestamp.is_some() {
            return Err(ServiceError::NotFound.into());
        }

        // next generation
        let generation = current.next_generation()?;

        let path = if current.finalizers.is_empty() {
            // no finalizers, we can directly delete
            accessor.delete(app_id, device_id).await?;

            "."
        } else {
            // update deleted timestamp
            current.deletion_timestamp = Some(Utc::now());

            // update the record
            accessor.update(current, None).await?;

            ".metadata"
        }
        .into();

        t.commit().await?;

        // send change event

        Event::new_device(
            self.instance.clone(),
            app_id,
            device_id,
            generation,
            vec![path],
        )
        .send_with(&self.sender)
        .await?;

        // done

        Ok(())
    }
}
