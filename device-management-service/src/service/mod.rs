pub mod admin;
mod error;
pub mod management;
mod utils;
mod x509;

use crate::{service::error::PostgresManagementServiceError, utils::epoch};
use deadpool_postgres::{Pool, Transaction};
use drogue_client::{registry, Translator};
use drogue_cloud_api_key_service::service::{KeycloakApiKeyService, KeycloakApiKeyServiceConfig};
use drogue_cloud_database_common::{
    auth::ensure,
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
    auth::user::{authz::Permission, UserInformation},
    health::{HealthCheckError, HealthChecked},
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use tokio_postgres::{error::SqlState, NoTls};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize)]
pub struct PostgresManagementServiceConfig {
    pub pg: deadpool_postgres::Config,
    pub instance: String,

    pub keycloak: KeycloakApiKeyServiceConfig,
}

impl<S> DatabaseService for PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait::async_trait]
impl<S> HealthChecked for PostgresManagementService<S>
where
    S: EventSender + Clone,
{
    async fn is_ready(&self) -> Result<(), HealthCheckError> {
        Ok(DatabaseService::is_ready(self)
            .await
            .map_err(HealthCheckError::from)?)
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

    keycloak: KeycloakApiKeyService,
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
            keycloak: KeycloakApiKeyService::new(config.keycloak)?,
        })
    }

    fn app_to_entity(
        mut app: registry::v1::Application,
    ) -> Result<
        (models::app::Application, HashSet<TypedAlias>),
        PostgresManagementServiceError<S::Error>,
    > {
        // extract aliases

        let mut aliases = HashSet::with_capacity(1);
        aliases.insert(TypedAlias("name".into(), app.metadata.name.clone()));

        // extract trust anchors

        match app.section::<registry::v1::ApplicationSpecTrustAnchors>() {
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

            owner: None,                 // will be set internally
            transfer_owner: None,        // will be set internally
            members: Default::default(), // will be set internally

            data: json!({
                "spec": app.spec,
                "status": app.status,
            }),
        };

        // return result

        Ok((app, aliases))
    }

    fn device_to_entity(
        device: registry::v1::Device,
    ) -> Result<
        (models::device::Device, HashSet<TypedAlias>),
        PostgresManagementServiceError<S::Error>,
    > {
        // extract aliases

        let mut aliases = HashSet::new();

        aliases.insert(TypedAlias("name".into(), device.metadata.name.clone()));

        if let Some(Ok(aliases_spec)) = device.section::<registry::v1::DeviceSpecAliases>() {
            for alias in aliases_spec.0 {
                aliases.insert(TypedAlias("alias".into(), alias));
            }
        }

        // extract credentials
        if let Some(Ok(credentials)) = device.section::<registry::v1::DeviceSpecCredentials>() {
            for credential in credentials.credentials {
                match credential {
                    registry::v1::Credential::UsernamePassword {
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
        identity: Option<&UserInformation>,
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
            ensure(&current, identity, Permission::Write)?;
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
            let generation = app.set_incremented_generation(&current)?;

            let name = app.name.clone();
            let uid = app.uid;

            // update

            accessor
                .update_data(app, aliases)
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
