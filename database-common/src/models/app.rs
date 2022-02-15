use crate::{
    auth::Resource,
    default_resource, diffable,
    error::ServiceError,
    generation,
    models::{
        fix_null_default,
        sql::{slice_iter, SelectBuilder},
        Lock, TypedAlias,
    },
    update_aliases, Client,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use core::pin::Pin;
use drogue_client::{meta, registry};
use drogue_cloud_service_api::{admin::Role, auth::user::UserInformation, labels::LabelSelector};
use futures::{future, Stream, TryStreamExt};
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{hash_map::RandomState, HashMap, HashSet};
use tokio_postgres::{
    types::{Json, ToSql, Type},
    Row,
};
use tracing::instrument;
use uuid::Uuid;

#[derive(Debug)]
/// An application entity record.
pub struct Application {
    pub uid: Uuid,
    pub name: String,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub creation_timestamp: DateTime<Utc>,
    pub resource_version: Uuid,
    pub generation: u64,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    pub finalizers: Vec<String>,

    /// ownership information
    pub owner: Option<String>,
    /// transfer to new owner
    pub transfer_owner: Option<String>,
    /// members list
    pub members: IndexMap<String, MemberEntry>,

    /// arbitrary payload
    pub data: Value,
}

diffable!(Application);
generation!(Application => generation);
default_resource!(Application);

impl Resource for Application {
    fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    fn members(&self) -> &IndexMap<String, MemberEntry> {
        &self.members
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberEntry {
    pub role: Role,
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

impl From<Application> for registry::v1::Application {
    fn from(app: Application) -> Self {
        let (app, spec) = extract_sect(app, "spec");
        let (app, status) = extract_sect(app, "status");

        registry::v1::Application {
            metadata: meta::v1::NonScopedMetadata {
                uid: app.uid.to_string(),
                name: app.name,
                labels: app.labels,
                annotations: app.annotations,
                creation_timestamp: app.creation_timestamp,
                generation: app.generation,
                resource_version: app.resource_version.to_string(),
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
    async fn delete(&self, app: &str) -> Result<(), ServiceError>;

    /// Get an application
    #[instrument(name = "database_app_lookup", skip(self, lock), err)]
    async fn get(&self, app: &str, lock: Lock) -> Result<Option<Application>, ServiceError> {
        Ok(self
            .list(
                Some(app),
                LabelSelector::default(),
                Some(1),
                None,
                None,
                lock,
                &[],
            )
            .await?
            .try_next()
            .await?)
    }

    /// Get a list of applications
    #[allow(clippy::too_many_arguments)]
    async fn list(
        &self,
        name: Option<&str>,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
        id: Option<&UserInformation>,
        lock: Lock,
        sort: &[&str],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Application, ServiceError>> + Send>>, ServiceError>;

    /// Create a new application
    async fn create(
        &self,
        application: Application,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError>;

    /// Update an existing application's data
    async fn update_data(
        &self,
        application: Application,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<u64, ServiceError>;

    /// Update an existing application's owner information
    async fn update_transfer(
        &self,
        app: String,
        owner: Option<String>,
        transfer_owner: Option<String>,
    ) -> Result<u64, ServiceError>;

    /// Set the member list
    async fn set_members(
        &self,
        app: &str,
        members: IndexMap<String, MemberEntry>,
    ) -> Result<u64, ServiceError>;
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
            uid: row.try_get("UID")?,
            name: row.try_get("NAME")?,

            creation_timestamp: row.try_get("CREATION_TIMESTAMP")?,
            generation: row.try_get::<_, i64>("GENERATION")? as u64,
            resource_version: row.try_get("RESOURCE_VERSION")?,
            labels: super::row_to_map(&row, "LABELS")?,
            annotations: super::row_to_map(&row, "ANNOTATIONS")?,
            deletion_timestamp: row.try_get("DELETION_TIMESTAMP")?,
            finalizers: super::row_to_vec(&row, "FINALIZERS")?,

            owner: row.try_get("OWNER")?,
            transfer_owner: row.try_get("TRANSFER_OWNER")?,
            members: row
                .try_get::<_, Json<IndexMap<String, MemberEntry>>>("MEMBERS")
                .map(|json| json.0)
                .or_else(fix_null_default)?,

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
            .prepare_typed(
                "INSERT INTO APPLICATION_ALIASES (APP, TYPE, ALIAS) VALUES ($1, $2, $3)",
                &[Type::VARCHAR, Type::VARCHAR, Type::VARCHAR],
            )
            .await?;

        for alias in aliases {
            self.client
                .execute(&stmt, &[&id, &alias.0, &alias.1])
                .await?;
        }

        Ok(())
    }
}

trait Param: ToSql + Sync {}

#[async_trait]
impl<'c, C: Client> ApplicationAccessor for PostgresApplicationAccessor<'c, C> {
    #[instrument(name = "database_app_alias_lookup", skip(self), err)]
    async fn lookup(&self, alias: &str) -> Result<Option<Application>, ServiceError> {
        let sql = r#"
SELECT
    A2.NAME,
    A2.UID,
    A2.LABELS,
    A2.CREATION_TIMESTAMP,
    A2.GENERATION,
    A2.RESOURCE_VERSION,
    A2.ANNOTATIONS,
    A2.DELETION_TIMESTAMP,
    A2.FINALIZERS,
    A2.OWNER,
    A2.TRANSFER_OWNER,
    A2.MEMBERS,
    A2.DATA
FROM
        APPLICATION_ALIASES A1 INNER JOIN APPLICATIONS A2
    ON
        A1.APP=A2.NAME WHERE A1.ALIAS = $1
"#;

        let stmt = self.client.prepare_typed(sql, &[Type::VARCHAR]).await?;
        let row = self.client.query_opt(&stmt, &[&alias]).await?;

        Ok(row.map(Self::from_row).transpose()?)
    }

    async fn delete(&self, id: &str) -> Result<(), ServiceError> {
        let sql = "DELETE FROM APPLICATIONS WHERE NAME = $1";
        let stmt = self.client.prepare_typed(sql, &[Type::VARCHAR]).await?;
        let count = self.client.execute(&stmt, &[&id]).await?;

        if count > 0 {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }

    async fn list(
        &self,
        name: Option<&str>,
        labels: LabelSelector,
        limit: Option<usize>,
        offset: Option<usize>,
        id: Option<&UserInformation>,
        lock: Lock,
        sort: &[&str],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Application, ServiceError>> + Send>>, ServiceError>
    {
        let select = r#"
SELECT
    NAME,
    UID,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    DELETION_TIMESTAMP,
    FINALIZERS,
    OWNER,
    TRANSFER_OWNER,
    MEMBERS,
    DATA
FROM APPLICATIONS
"#
        .to_string();

        let builder = SelectBuilder::new(select, Vec::new(), Vec::new())
            .name(&name)
            .labels(&labels.0)
            .auth_read(&id)
            .lock(lock)
            .sort(sort)
            .limit(limit)
            .offset(offset);

        let (select, params, types) = builder.build();

        let stmt = self.client.prepare_typed(&select, &types).await?;

        let stream = self
            .client
            .query_raw(&stmt, slice_iter(&params[..]))
            .await
            .map_err(|err| {
                log::debug!("Failed to get: {}", err);
                err
            })?
            .and_then(|row| future::ready(Self::from_row(row)))
            .map_err(ServiceError::Database);

        Ok(Box::pin(stream))
    }

    async fn create(
        &self,
        application: Application,
        aliases: HashSet<TypedAlias>,
    ) -> Result<(), ServiceError> {
        let name = application.name;
        let data = application.data;
        let labels = application.labels;
        let annotations = application.annotations;

        self.client
            .execute(
                r#"
INSERT INTO APPLICATIONS (
    NAME,
    UID,
    LABELS,
    ANNOTATIONS,
    CREATION_TIMESTAMP,
    GENERATION,
    RESOURCE_VERSION,
    FINALIZERS,
    OWNER,
    DATA
) VALUES (
    $1,
    $2,
    $3,
    $4,
    $5,
    $6,
    $7,
    $8,
    $9,
    $10
)"#,
                &[
                    &name,
                    &application.uid,
                    &Json(labels),
                    &Json(annotations),
                    &Utc::now(),
                    &(application.generation as i64),
                    &Uuid::new_v4(),
                    &application.finalizers,
                    &application.owner,
                    &Json(data),
                ],
            )
            .await?;

        self.insert_aliases(&name, &aliases).await?;

        Ok(())
    }

    async fn update_data(
        &self,
        application: Application,
        aliases: Option<HashSet<TypedAlias>>,
    ) -> Result<u64, ServiceError> {
        let name = application.name;
        let labels = application.labels;
        let data = application.data;
        let annotations = application.annotations;

        // update device
        let count = self
            .client
            .execute(
                r#"
UPDATE APPLICATIONS
SET
    LABELS = $2,
    ANNOTATIONS = $3,
    GENERATION = $4,
    RESOURCE_VERSION = $5,
    DELETION_TIMESTAMP = $6,
    FINALIZERS = $7,
    DATA = $8
WHERE
    NAME = $1
"#,
                &[
                    &name,
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

            let sql = "DELETE FROM APPLICATION_ALIASES WHERE APP=$1";
            let stmt = self.client.prepare_typed(sql, &[Type::VARCHAR]).await?;

            self.client.execute(&stmt, &[&name]).await?;

            // insert new alias set
            self.insert_aliases(&name, &aliases).await?;

            Ok(count)
        })
    }

    async fn update_transfer(
        &self,
        app: String,
        owner: Option<String>,
        transfer_owner: Option<String>,
    ) -> Result<u64, ServiceError> {
        // update application
        let sql = r#"
UPDATE APPLICATIONS
SET
    OWNER = $2,
    TRANSFER_OWNER = $3
WHERE
    NAME = $1
"#;

        let stmt = self
            .client
            .prepare_typed(sql, &[Type::VARCHAR, Type::VARCHAR, Type::VARCHAR])
            .await?;
        let count = self
            .client
            .execute(&stmt, &[&app, &owner, &transfer_owner])
            .await?;

        Ok(count)
    }

    async fn set_members(
        &self,
        app: &str,
        members: IndexMap<String, MemberEntry>,
    ) -> Result<u64, ServiceError> {
        // update application

        let sql = r#"
UPDATE APPLICATIONS
SET
    MEMBERS = $2
WHERE
    NAME = $1
"#;

        let stmt = self
            .client
            .prepare_typed(sql, &[Type::VARCHAR, Type::JSONB])
            .await?;

        let count = self.client.execute(&stmt, &[&app, &Json(members)]).await?;

        Ok(count)
    }
}
