use super::utils;
use crate::{
    endpoints::params::DeleteParams,
    service::{error::PostgresManagementServiceError, PostgresManagementService},
};
use async_trait::async_trait;
use chrono::Utc;
use core::pin::Pin;
use drogue_client::registry;
use drogue_cloud_database_common::{
    auth::{ensure, ensure_with},
    error::ServiceError,
    models::{
        app::{ApplicationAccessor, PostgresApplicationAccessor},
        device::{DeviceAccessor, PostgresDeviceAccessor},
        diff::diff_paths,
        Advance, Lock,
    },
};
use drogue_cloud_registry_events::{Event, EventSender, SendEvent};
use drogue_cloud_service_api::{
    auth::user::{authz::Permission, UserInformation},
    labels::LabelSelector,
    webapp::ResponseError,
};
use drogue_cloud_service_common::keycloak::KeycloakClient;
use futures::{future, Stream, TryStreamExt};
use tokio_postgres::error::SqlState;
use uuid::Uuid;

#[async_trait]
pub trait ManagementService: Clone {
    type Error: ResponseError;

    async fn create_app(
        &self,
        identity: &UserInformation,
        data: registry::v1::Application,
    ) -> Result<(), Self::Error>;

    async fn get_app(
        &self,
        identity: &UserInformation,
        name: &str,
    ) -> Result<Option<registry::v1::Application>, Self::Error>;

    async fn list_apps(
        &self,
        identity: UserInformation,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<registry::v1::Application, Self::Error>> + Send>>,
        Self::Error,
    >;

    async fn update_app(
        &self,
        identity: &UserInformation,
        data: registry::v1::Application,
    ) -> Result<(), Self::Error>;

    async fn delete_app(
        &self,
        identity: &UserInformation,
        name: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error>;

    async fn create_device(
        &self,
        identity: &UserInformation,
        device: registry::v1::Device,
    ) -> Result<(), Self::Error>;

    async fn get_device(
        &self,
        identity: &UserInformation,
        app: &str,
        name: &str,
    ) -> Result<Option<registry::v1::Device>, Self::Error>;

    async fn list_devices(
        &self,
        identity: UserInformation,
        app: &str,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<registry::v1::Device, Self::Error>> + Send>>,
        Self::Error,
    >;

    async fn update_device(
        &self,
        identity: &UserInformation,
        device: registry::v1::Device,
    ) -> Result<(), Self::Error>;

    async fn delete_device(
        &self,
        identity: &UserInformation,
        app: &str,
        name: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error>;
}

#[async_trait]
impl<S, K> ManagementService for PostgresManagementService<S, K>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    type Error = PostgresManagementServiceError<S::Error>;

    async fn create_app(
        &self,
        identity: &UserInformation,
        application: registry::v1::Application,
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
        identity: &UserInformation,
        name: &str,
    ) -> Result<Option<registry::v1::Application>, Self::Error> {
        let c = self.pool.get().await?;

        let app = PostgresApplicationAccessor::new(&c)
            .get(name, Lock::None)
            .await?;

        if let Some(app) = &app {
            ensure(app, identity, Permission::Read)?;
        }

        Ok(app.map(Into::into))
    }

    async fn list_apps(
        &self,
        identity: UserInformation,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<registry::v1::Application, Self::Error>> + Send>>,
        Self::Error,
    > {
        let c = self.pool.get().await?;

        Ok(Box::pin(
            PostgresApplicationAccessor::new(&c)
                .list(
                    None,
                    labels,
                    limit,
                    offset,
                    Some(&identity),
                    Lock::None,
                    &["NAME"],
                )
                .await?
                .try_filter_map(move |app| {
                    // Using ensure call here is just a safeguard! The list operation must only return
                    // entries the user has access to. Otherwise the limit/offset functionality
                    // won't work
                    let result = match ensure(&app, &identity, Permission::Read) {
                        Ok(_) => Some(app.into()),
                        Err(_) => None,
                    };
                    future::ready(Ok(result))
                })
                .map_err(PostgresManagementServiceError::Service)
                .into_stream(),
        ))
    }

    async fn update_app(
        &self,
        identity: &UserInformation,
        application: registry::v1::Application,
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
        identity: &UserInformation,
        id: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresApplicationAccessor::new(&t);

        // get current state for diffing
        let mut current = match accessor.get(id, Lock::ForUpdate).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        if current.deletion_timestamp.is_some() {
            return Err(PostgresManagementServiceError::Service(
                ServiceError::NotFound,
            ));
        }

        ensure(&current, identity, Permission::Admin)?;

        utils::check_preconditions(&params.preconditions, &current)?;
        // there is no need to use the provided constraints, we as locked the entry "for update"

        // next, we need to delete the application

        // first, delete all devices ...
        let remaining_devices = PostgresDeviceAccessor::new(&t).delete_app(id).await?;

        // ...and count the once we can only soft-delete
        if remaining_devices > 0 {
            // we have pending device deletions, so add the finalizer
            current.finalizers.push("has-devices".into());
        }

        // next generation
        let revision = current.advance_revision()?;
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
            accessor.update_data(current, None).await?;

            // notify a resource change
            vec![".metadata".into()]
        };

        // create events

        let events = Event::new_app(self.instance.clone(), id, uid, revision, paths);

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
        identity: &UserInformation,
        device: registry::v1::Device,
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
        ensure_with(&app, identity, Permission::Write, || {
            ServiceError::ReferenceNotFound
        })?;

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
        identity: &UserInformation,
        app_id: &str,
        device_id: &str,
    ) -> Result<Option<registry::v1::Device>, Self::Error> {
        let c = self.pool.get().await?;

        let app = PostgresApplicationAccessor::new(&c)
            .get(app_id, Lock::None)
            .await?
            .ok_or(ServiceError::NotFound)?;

        // ensure we have access, but don't confirm the device if we don't
        ensure_with(&app, identity, Permission::Read, || ServiceError::NotFound)?;

        let device = PostgresDeviceAccessor::new(&c)
            .get(app_id, device_id, Lock::None)
            .await?;

        Ok(device.map(Into::into))
    }

    async fn list_devices(
        &self,
        identity: UserInformation,
        app_id: &str,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<registry::v1::Device, Self::Error>> + Send>>,
        Self::Error,
    > {
        let c = self.pool.get().await?;

        let app = PostgresApplicationAccessor::new(&c)
            .get(app_id, Lock::None)
            .await?
            .ok_or(ServiceError::NotFound)?;

        // ensure we have access, but don't confirm the device if we don't
        ensure_with(&app, &identity, Permission::Read, || ServiceError::NotFound)?;

        Ok(Box::pin(
            PostgresDeviceAccessor::new(&c)
                .list(app_id, None, labels, limit, offset, Lock::None)
                .await?
                .map_ok(|device| device.into())
                .map_err(PostgresManagementServiceError::Service)
                .into_stream(),
        ))
    }

    async fn update_device(
        &self,
        identity: &UserInformation,
        device: registry::v1::Device,
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
        ensure_with(&current, identity, Permission::Write, || {
            ServiceError::NotFound
        })?;

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

            let revision = device.advance_from(&paths, &current)?;
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
                revision,
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
        identity: &UserInformation,
        application: &str,
        device: &str,
        params: DeleteParams,
    ) -> Result<(), Self::Error> {
        let mut c = self.pool.get().await?;
        let t = c.build_transaction().start().await?;

        let accessor = PostgresDeviceAccessor::new(&t);

        // get current state for diffing
        let mut current = match accessor.get(application, device, Lock::ForUpdate).await? {
            Some(device) => Ok(device),
            None => Err(ServiceError::NotFound),
        }?;

        if current.deletion_timestamp.is_some() {
            return Err(PostgresManagementServiceError::Service(
                ServiceError::NotFound,
            ));
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
        ensure_with(&app, identity, Permission::Write, || ServiceError::NotFound)?;

        // check the preconditions
        utils::check_preconditions(&params.preconditions, &current)?;
        // there is no need to use the provided constraints, we as locked the entry "for update"

        // next generation
        let revision = current.advance_revision()?;
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
            revision,
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
