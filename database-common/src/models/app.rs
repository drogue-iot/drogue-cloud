use crate::{error::ServiceError, models::TypedAlias, Client};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use drogue_cloud_service_api::management::{self, NonScopedMetadata};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
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

    pub data: Value,
}

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
    async fn delete(&self, id: &str) -> Result<bool, ServiceError>;

    /// Get an application
    async fn get(&self, id: &str) -> Result<Option<Application>, ServiceError>;

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
        aliases: HashSet<TypedAlias>,
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
            id: row.try_get::<_, String>("ID")?,

            creation_timestamp: row.try_get::<_, DateTime<Utc>>("CREATION_TIMESTAMP")?,
            generation: row.try_get::<_, i64>("GENERATION")? as u64,
            resource_version: row.try_get::<_, Uuid>("RESOURCE_VERSION")?.to_string(),
            labels: super::row_to_map(&row, "LABELS")?,
            annotations: super::row_to_map(&row, "ANNOTATIONS")?,

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
    A2.ID, A2.LABELS, A2.DATA
FROM APPLICATION_ALIASES A1 INNER JOIN APPLICATIONS A2
    ON A1.ID=A2.ID WHERE A1.ALIAS = $1
"#,
                &[&alias],
            )
            .await?;

        Ok(row.map(Self::from_row).transpose()?)
    }

    async fn delete(&self, id: &str) -> Result<bool, ServiceError> {
        let count = self
            .client
            .execute("DELETE FROM APPLICATIONS WHERE ID = $1", &[&id])
            .await?;

        Ok(count > 0)
    }

    async fn get(&self, id: &str) -> Result<Option<Application>, ServiceError> {
        let result = self
            .client
            .query_opt(
                r#"
SELECT
    ID,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    DATA
FROM APPLICATIONS
    WHERE ID = $1"#,
                &[&id],
            )
            .await?
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
    DATA
) VALUES (
    $1,
    $2,
    $3,
    $4,
    0,
    $5,
    $6
)"#,
                &[
                    &id,
                    &Json(labels),
                    &Json(annotations),
                    &Utc::now(),
                    &Uuid::new_v4(),
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
        aliases: HashSet<TypedAlias>,
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
    GENERATION = GENERATION + 1,
    RESOURCE_VERSION = $4,
    DATA = $5
WHERE
    ID = $1
"#,
                &[
                    &id,
                    &Json(labels),
                    &Json(annotations),
                    &Uuid::new_v4(),
                    &Json(data),
                ],
            )
            .await?;

        // did we update something?
        if count > 0 {
            // clear existing aliases
            self.client
                .execute("DELETE FROM APPLICATION_ALIASES WHERE ID=$1", &[&id])
                .await?;

            // insert new alias set
            self.insert_aliases(&id, &aliases).await?;

            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }
}
