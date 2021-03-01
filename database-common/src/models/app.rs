use crate::{
    diffable,
    error::ServiceError,
    generation,
    models::{Lock, TypedAlias},
    update_aliases, Client,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use drogue_cloud_service_api::management::{self, NonScopedMetadata};
use serde_json::{Map, Value};
use std::collections::{hash_map::RandomState, HashMap, HashSet};
use tokio_postgres::{types::Json, Row};
use uuid::Uuid;

/// An application entity record.
pub struct Application {
    pub id: String,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub creation_timestamp: DateTime<Utc>,
    pub resource_version: String,
    pub generation: u64,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    pub finalizers: Vec<String>,

    pub data: Value,
}

diffable!(Application);
generation!(Application => generation);

/// Extract a section from the application data. Prevents cloning the whole struct.
fn extract_sect(mut app: Application, key: &str) -> (Application, Option<Map<String, Value>>) {
    let sect = app
        .data
        .get_mut(key)
        .map(|v| v.take())
        .and_then(|v| match v {
            Value::Object(v) => Some(v),
            _ => None,
        });

    (app, sect)
}

impl From<Application> for management::Application {
    fn from(app: Application) -> Self {
        let (app, spec) = extract_sect(app, "spec");
        let (app, status) = extract_sect(app, "status");

        management::Application {
            metadata: NonScopedMetadata {
                name: app.id,
                labels: app.labels,
                annotations: app.annotations,
                creation_timestamp: app.creation_timestamp,
                generation: app.generation,
                resource_version: app.resource_version,
                deletion_timestamp: app.deletion_timestamp,
                finalizers: app.finalizers,
            },
            spec: spec.unwrap_or_default(),
            status: status.unwrap_or_default(),
        }
    }
}

#[async_trait]
pub trait ApplicationAccessor {
    /// Lookup an application
    async fn lookup(&self, alias: &str) -> Result<Option<Application>, ServiceError>;

    /// Delete an application
    async fn delete(&self, id: &str) -> Result<(), ServiceError>;

    /// Get an application
    async fn get(&self, id: &str, lock: Lock) -> Result<Option<Application>, ServiceError>;

    /// Create a new application
    async fn create(
        &self,
        application: Application,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError>;

    /// Update an existing application
    async fn update(
        &self,
        application: Application,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<(), ServiceError>;
}

pub struct PostgresApplicationAccessor<'c, C: Client> {
    client: &'c C,
}

impl<'c, C: Client> PostgresApplicationAccessor<'c, C> {
    pub fn new(client: &'c C) -> Self {
        Self { client }
    }

    pub fn from_row(row: Row) -> Result<Application, tokio_postgres::Error> {
        log::debug!("Row: {:?}", row);
        Ok(Application {
            id: row.try_get("ID")?,

            creation_timestamp: row.try_get("CREATION_TIMESTAMP")?,
            generation: row.try_get::<_, i64>("GENERATION")? as u64,
            resource_version: row.try_get::<_, Uuid>("RESOURCE_VERSION")?.to_string(),
            labels: super::row_to_map(&row, "LABELS")?,
            annotations: super::row_to_map(&row, "ANNOTATIONS")?,
            deletion_timestamp: row.try_get("DELETION_TIMESTAMP")?,
            finalizers: super::row_to_vec(&row, "FINALIZERS")?,

            data: row.try_get::<_, Json<_>>("DATA")?.0,
        })
    }

    async fn insert_aliases(
        &self,
        id: &str,
        aliases: &HashSet<TypedAlias>,
    ) -> Result<(), tokio_postgres::Error> {
        if aliases.is_empty() {
            return Ok(());
        }

        let stmt = self
            .client
            .prepare("INSERT INTO APPLICATION_ALIASES (ID, TYPE, ALIAS) VALUES ($1, $2, $3)")
            .await?;

        for alias in aliases {
            self.client
                .execute(&stmt, &[&id, &alias.0, &alias.1])
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl<'c, C: Client> ApplicationAccessor for PostgresApplicationAccessor<'c, C> {
    async fn lookup(&self, alias: &str) -> Result<Option<Application>, ServiceError> {
        let row = self
            .client
            .query_opt(
                r#"
SELECT
    A2.ID, A2.LABELS, A2.CREATION_TIMESTAMP, A2.GENERATION, A2.RESOURCE_VERSION, A2.ANNOTATIONS, A2.DELETION_TIMESTAMP, A2.FINALIZERS, A2.DATA
FROM APPLICATION_ALIASES A1 INNER JOIN APPLICATIONS A2
    ON A1.ID=A2.ID WHERE A1.ALIAS = $1
"#,
                &[&alias],
            )
            .await?;

        Ok(row.map(Self::from_row).transpose()?)
    }

    async fn delete(&self, id: &str) -> Result<(), ServiceError> {
        let count = self
            .client
            .execute("DELETE FROM APPLICATIONS WHERE ID = $1", &[&id])
            .await?;

        if count > 0 {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }

    async fn get(&self, id: &str, lock: Lock) -> Result<Option<Application>, ServiceError> {
        let result = self
            .client
            .query_opt(
                format!(
                    r#"
SELECT
    ID,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    DELETION_TIMESTAMP,
    FINALIZERS,
    DATA
FROM APPLICATIONS
    WHERE ID = $1
{for_update}
"#,
                    for_update = lock.to_string()
                )
                .as_str(),
                &[&id],
            )
            .await
            .map_err(|err| {
                log::debug!("Failed to get: {}", err);
                err
            })?
            .map(Self::from_row)
            .transpose()?;

        Ok(result)
    }

    async fn create(
        &self,
        application: Application,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError> {
        let id = application.id;
        let data = application.data;
        let labels = application.labels;
        let annotations = application.annotations;

        self.client
            .execute(
                r#"
INSERT INTO APPLICATIONS (
    ID,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    DELETION_TIMESTAMP,
    FINALIZERS,
    DATA
) VALUES (
    $1,
    $2,
    $3,
    $4,
    $5,
    $6,
    NULL,
    $7,
    $8
)"#,
                &[
                    &id,
                    &Json(labels),
                    &Json(annotations),
                    &Utc::now(),
                    &(application.generation as i64),
                    &Uuid::new_v4(),
                    &application.finalizers,
                    &Json(data),
                ],
            )
            .await?;

        self.insert_aliases(&id, &aliases).await?;

        Ok(())
    }

    async fn update(
        &self,
        application: Application,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<(), ServiceError> {
        let id = application.id;
        let labels = application.labels;
        let data = application.data;
        let annotations = application.annotations;

        // update device
        let count = self
            .client
            .execute(
                r#"
UPDATE APPLICATIONS SET
    LABELS = $2,
    ANNOTATIONS = $3,
    GENERATION = $4,
    RESOURCE_VERSION = $5,
    DELETION_TIMESTAMP = $6,
    FINALIZERS = $7,
    DATA = $8
WHERE
    ID = $1
"#,
                &[
                    &id,
                    &Json(labels),
                    &Json(annotations),
                    &(application.generation as i64),
                    &Uuid::new_v4(),
                    &application.deletion_timestamp,
                    &application.finalizers,
                    &Json(data),
                ],
            )
            .await?;

        update_aliases!(count, aliases, |aliases| {
            // clear existing aliases
            self.client
                .execute("DELETE FROM APPLICATION_ALIASES WHERE ID=$1", &[&id])
                .await?;

            // insert new alias set
            self.insert_aliases(&id, &aliases).await?;

            Ok(())
        })
    }
}
