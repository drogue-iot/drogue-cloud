mod authn;
mod error;
mod utils;
mod x509;

use crate::endpoints::params::DeleteParams;
use crate::service::authn::{ensure, ensure_with};
use crate::{service::error::PostgresManagementServiceError, utils::epoch};
use actix_web::ResponseError;
use async_trait::async_trait;
use chrono::Utc;
use deadpool_postgres::{Pool, Transaction};
use drogue_cloud_database_common::{
    error::ServiceError,
    models::{
        self,
        app::{ApplicationAccessor, PostgresApplicationAccessor},
        device::{DeviceAccessor, PostgresDeviceAccessor},
        diff::diff_paths,
        outbox::PostgresOutboxAccessor,
        Generation, Lock, TypedAlias,
    },
    Client, DatabaseService,
};
use drogue_cloud_registry_events::{Event, EventSender, EventSenderError, SendEvent};
use drogue_cloud_service_api::{
    health::HealthCheckedService,
    management::{
        Application, ApplicationSpecTrustAnchors, Credential, Device, DeviceSpecCredentials,
    },
    Translator,
};
use drogue_cloud_service_common::auth::Identity;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use tokio_postgres::{error::SqlState, NoTls};
use uuid::Uuid;

#[async_trait]
pub trait ManagementService: HealthCheckedService + Clone {
    type Error: ResponseError;

    async fn create_app(
        &self,
        identity: &dyn Identity,
        data: Application,
    ) -> Result<(), Self::Error>;
    async fn get_app(
        &self,
        identity: &dyn Identity,
        name: &str,
    ) -> Result<Option<Application>, Self::Error>;
    async fn update_app(
        &self,
        identity: &dyn Identity,
        data: Application,
    ) -> Result<(), Self::Error>;
    async fn delete_app(
        &self,
        identity: &dyn Identity,
        name: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error>;

    async fn create_device(
        &self,
        identity: &dyn Identity,
        device: Device,
    ) -> Result<(), Self::Error>;
    async fn get_device(
        &self,
        identity: &dyn Identity,
        app: &str,
        name: &str,
    ) -> Result<Option<Device>, Self::Error>;
    async fn update_device(
        &self,
        identity: &dyn Identity,
        device: Device,
    ) -> Result<(), Self::Error>;
    async fn delete_device(
        &self,
        identity: &dyn Identity,
        app: &str,
        name: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct PostgresManagementServiceConfig {
    pub pg: deadpool_postgres::Config,
    pub instance: String,
}

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
        aliases.insert(TypedAlias("name".into(), app.metadata.name.clone()));

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
            name: app.metadata.name,
            uid: Uuid::nil(), // will be set internally
            labels: app.metadata.labels,
            annotations: app.metadata.annotations,
            generation: 0,                 // will be set internally
            creation_timestamp: epoch(),   // will be set internally
            resource_version: Uuid::nil(), // will be set internally
            deletion_timestamp: None,      // will be set internally
            finalizers: app.metadata.finalizers,

            owner: None, // will be set internally

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

        aliases.insert(TypedAlias("name".into(), device.metadata.name.clone()));

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
            name: device.metadata.name,
            uid: Uuid::nil(), // will be set internally
            application: device.metadata.application,
            labels: device.metadata.labels,
            annotations: device.metadata.annotations,
            creation_timestamp: epoch(),   // will be set internally
            generation: 0,                 // will be set internally
            resource_version: Uuid::nil(), // will be set internally
            deletion_timestamp: None,      // will be set internally
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
    async fn perform_update_app<S1, S2>(
        &self,
        t: &Transaction<'_>,
        identity: Option<&dyn Identity>,
        mut app: models::app::Application,
        aliases: Option<HashSet<TypedAlias>>,
        expected_uid: S1,
        expected_resource_version: S2,
    ) -> Result<Vec<Event>, PostgresManagementServiceError<S::Error>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let accessor = PostgresApplicationAccessor::new(t);

        // get current state for diffing
        let current = match accessor.get(&app.name, Lock::ForUpdate).await? {
            Some(app) => Ok(app),
            None => Err(ServiceError::NotFound),
        }?;

        if let Some(identity) = identity {
            ensure(&current, identity)?;
        }

        utils::check_versions(expected_uid, expected_resource_version, &current)?;

        // we simply copy over the deletion timestamp
        app.deletion_timestamp = current.deletion_timestamp;

        if app.deletion_timestamp.is_some() && app.finalizers.is_empty() {
            // delete, but don't send any event
            accessor.delete(&app.name).await?;

            Ok(vec![])
        } else {
            // check which paths changed

            let paths = diff_paths(&current, &app);
            if paths.is_empty() {
                // there was no change
                return Ok(vec![]);
            }

            // next generation
            let generation = app.next_generation(&current)?;

            let name = app.name.clone();
            let uid = app.uid;

            // update

            accessor
                .update(app, aliases)
                .await
                .map_err(|err| match err.sql_state() {
                    Some(state) if state == &SqlState::UNIQUE_VIOLATION => {
                        ServiceError::Conflict("Unique key violation".to_string())
                    }
                    _ => err,
                })?;

            // send change event

            Ok(Event::new_app(
                self.instance.clone(),
                name,
                uid,
                generation,
                paths,
            ))
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
        self.perform_update_app(t, None, app, None, "", "").await?;

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

    async fn send_to_outbox<'c, C: Client, E>(
        client: &C,
        events: &[Event],
    ) -> Result<(), PostgresManagementServiceError<E>>
    where
        E: std::error::Error + std::fmt::Debug + 'static,
    {
        // send events to outbox

        events
            .to_vec()
            .send_with(&PostgresOutboxAccessor::new(client))
            .await
            .map_err(Self::outbox_err)
    }
}

#[async_trait]
impl<S> ManagementService for PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    type Error = PostgresManagementServiceError<S::Error>;

    async fn create_app(
        &self,
        identity: &dyn Identity,
        application: Application,
    ) -> Result<(), Self::Error> {
        let (mut app, aliases) = Self::app_to_entity(application)?;

        let generation = app.generation;
        let name = app.name.clone();
        // assign a new UID
        let uid = Uuid::new_v4();
        app.uid = uid;
        app.owner = identity.user_id().map(Into::into);

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        PostgresApplicationAccessor::new(&t)
            .create(app, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => {
                    ServiceError::Conflict("Unique key violation".to_string())
                }
                _ => err,
            })?;

        let events = Event::new_app(self.instance.clone(), name, uid, generation, vec![]);

        // send events to outbox

        Self::send_to_outbox(&t, &events).await?;

        // commit

        t.commit().await?;

        // send change events

        events.send_with(&self.sender).await?;

        // done

        Ok(())
    }

    async fn get_app(
        &self,
        identity: &dyn Identity,
        name: &str,
    ) -> Result<Option<Application>, Self::Error> {
        let c = self.pool.get().await?;

        let app = PostgresApplicationAccessor::new(&c)
            .get(name, Lock::None)
            .await?;

        if let Some(app) = &app {
            ensure(app, identity)?;
        }

        Ok(app.map(Into::into))
    }

    async fn update_app(
        &self,
        identity: &dyn Identity,
        application: Application,
    ) -> Result<(), Self::Error> {
        let expected_uid = application.metadata.uid.clone();
        let expected_resource_version = application.metadata.resource_version.clone();

        let (app, aliases) = Self::app_to_entity(application)?;

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let events = self
            .perform_update_app(
                &t,
                Some(identity),
                app,
                Some(aliases),
                expected_uid,
                expected_resource_version,
            )
            .await?;

        Self::send_to_outbox(&t, &events).await?;

        t.commit().await?;

        // send events

        events.send_with(&self.sender).await?;

        Ok(())
    }

    async fn delete_app(
        &self,
        identity: &dyn Identity,
        id: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error> {
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

        //
        ensure(&current, identity)?;

        utils::check_preconditions(&params.preconditions, &current)?;
        // there is no need to use the provided constraints, we as locked the entry "for update"

        // next, we need to delete the application

        // first, delete all devices ...
        let remaining_devices = PostgresDeviceAccessor::new(&t).delete_app(&id).await?;

        // ...and count the once we can only soft-delete
        if remaining_devices > 0 {
            // we have pending device deletions, so add the finalizer
            current.finalizers.push("has-devices".into());
        }

        // next generation
        let generation = current.set_next_generation()?;
        let uid = current.uid;

        // if there are no finalizers ...
        let paths = if current.finalizers.is_empty() {
            // ... delete the application
            accessor.delete(id).await?;

            // notify an object change
            vec![]
        } else {
            // ... otherwise, mark the application deleted
            log::debug!("Pending finalizers: {:?}", current.finalizers);

            // update deleted timestamp
            current.deletion_timestamp = Some(Utc::now());

            // update the record
            accessor.update(current, None).await?;

            // notify a resource change
            vec![".metadata".into()]
        };

        // create events

        let events = Event::new_app(self.instance.clone(), id, uid, generation, paths);

        // send events to outbox

        Self::send_to_outbox(&t, &events).await?;

        // commit

        t.commit().await?;

        // send change event

        events.send_with(&self.sender).await?;

        // done

        Ok(())
    }

    async fn create_device(
        &self,
        identity: &dyn Identity,
        device: Device,
    ) -> Result<(), Self::Error> {
        let (mut device, aliases) = Self::device_to_entity(device)?;

        let generation = device.generation;

        let application = device.application.clone();

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let app = PostgresApplicationAccessor::new(&t)
            .get(&application, Lock::ForShare)
            .await?;

        // if there is no entry, or it is marked for deletion, we don't allow adding a new device

        let app = match app {
            Some(app) if app.deletion_timestamp.is_none() => app,
            _ => return Err(ServiceError::ReferenceNotFound.into()),
        };

        // ensure we have access to the application, but don't confirm the device if we don't
        ensure_with(&app, identity, || ServiceError::ReferenceNotFound)?;

        let name = device.name.clone();
        // assign a new UID
        let uid = Uuid::new_v4();
        device.uid = uid;

        // create the device

        PostgresDeviceAccessor::new(&t)
            .create(device, aliases)
            .await
            .map_err(|err| match err.sql_state() {
                Some(state) if state == &SqlState::UNIQUE_VIOLATION => {
                    ServiceError::Conflict("Unique key violation".to_string())
                }
                Some(state) if state == &SqlState::FOREIGN_KEY_VIOLATION => {
                    ServiceError::ReferenceNotFound
                }
                _ => err,
            })?;

        // create and persist events

        let events = Event::new_device(
            self.instance.clone(),
            application,
            name,
            uid,
            generation,
            vec![],
        );

        // send events to outbox

        Self::send_to_outbox(&t, &events).await?;

        t.commit().await?;

        // send change events

        events.send_with(&self.sender).await?;

        // done

        Ok(())
    }

    async fn get_device(
        &self,
        identity: &dyn Identity,
        app_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, Self::Error> {
        let c = self.pool.get().await?;

        let app = PostgresApplicationAccessor::new(&c)
            .get(app_id, Lock::None)
            .await?
            .ok_or(ServiceError::NotFound)?;

        // ensure we have access, but don't confirm the device if we don't
        ensure_with(&app, identity, || ServiceError::NotFound)?;

        let device = PostgresDeviceAccessor::new(&c)
            .get(app_id, device_id, Lock::None)
            .await?;

        Ok(device.map(Into::into))
    }

    async fn update_device(
        &self,
        identity: &dyn Identity,
        device: Device,
    ) -> Result<(), Self::Error> {
        let expected_resource_version = device.metadata.resource_version.clone();
        let expected_uid = device.metadata.uid.clone();

        let (mut device, aliases) = Self::device_to_entity(device)?;

        let application = device.application.clone();
        let name = device.name.clone();

        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        let current = match accessor.get(&application, Lock::None).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        // ensure we have access, but don't confirm the device if we don't
        ensure_with(&current, identity, || ServiceError::NotFound)?;

        let accessor = PostgresDeviceAccessor::new(&t);

        // get current state for diffing
        let current = match accessor.get(&application, &name, Lock::ForUpdate).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        // pre-check versions
        utils::check_versions(expected_uid, expected_resource_version, &current)?;

        // we simply copy over the deletion timestamp
        device.deletion_timestamp = current.deletion_timestamp;

        if device.deletion_timestamp.is_some() && device.finalizers.is_empty() {
            // delete, but don't send any event
            accessor.delete(&application, &name).await?;

            // check with the application
            self.check_clean_app(&t, &application).await?;

            t.commit().await?;
        } else {
            // check which paths changed
            let paths = diff_paths(&current, &device);
            if paths.is_empty() {
                // there was no change
                return Ok(());
            }

            let generation = device.next_generation(&current)?;
            let uid = current.uid;

            accessor
                .update(device, Some(aliases))
                .await
                .map_err(|err| match err.sql_state() {
                    Some(state) if state == &SqlState::UNIQUE_VIOLATION => {
                        ServiceError::Conflict("Unique key violation".to_string())
                    }
                    _ => err,
                })?;

            // create events

            let events = Event::new_device(
                self.instance.clone(),
                application,
                name,
                uid,
                generation,
                paths,
            );

            // send events to outbox

            Self::send_to_outbox(&t, &events).await?;

            // commit

            t.commit().await?;

            // send change event

            events.send_with(&self.sender).await?;
        }

        // done

        Ok(())
    }

    async fn delete_device(
        &self,
        identity: &dyn Identity,
        application: &str,
        device: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresDeviceAccessor::new(&t);

        // get current state for diffing
        let mut current = match accessor.get(&application, &device, Lock::ForUpdate).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        if current.deletion_timestamp.is_some() {
            return Err(ServiceError::NotFound.into());
        }

        // check if the user has access to the device, we can do this after some initial checks
        // that would return "not found" anyway.
        // Instead of "no access" we return "not found" here, as we don't want users that don't
        // have access to application to probe for devices.
        let app = PostgresApplicationAccessor::new(&t)
            .get(application, Lock::None)
            .await?
            .ok_or(ServiceError::NotFound)?;

        // ensure we have access, but don't confirm the device if we don't
        ensure_with(&app, identity, || ServiceError::NotFound)?;

        // check the preconditions
        utils::check_preconditions(&params.preconditions, &current)?;
        // there is no need to use the provided constraints, we as locked the entry "for update"

        // next generation
        let generation = current.set_next_generation()?;
        let uid = current.uid;

        // if there are no finalizers ...
        let path = if current.finalizers.is_empty() {
            // ... we can directly delete
            accessor.delete(application, device).await?;

            vec![]
        } else {
            // ... otherwise, mark the device deleted
            log::debug!("Pending finalizers: {:?}", current.finalizers);

            // update deleted timestamp
            current.deletion_timestamp = Some(Utc::now());

            // update the record
            accessor.update(current, None).await?;

            vec![".metadata".into()]
        };

        // create events

        let events = Event::new_device(
            self.instance.clone(),
            application,
            device,
            uid,
            generation,
            path,
        );

        // send events to outbox

        Self::send_to_outbox(&t, &events).await?;

        // commit

        t.commit().await?;

        // send change events

        events.send_with(&self.sender).await?;

        // done

        Ok(())
    }
}
