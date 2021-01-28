use crate::{error::ServiceError, models::TypedAlias, Client};
use async_trait::async_trait;
use drogue_cloud_service_api::management::{self, ApplicationMetadata};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tokio_postgres::{types::Json, Row};

/// An application entity record.
pub struct Application {
    pub id: String,
    pub labels: HashMap<String, String>,
    pub data: Value,
}

impl From<Application> for management::Application {
    fn from(app: Application) -> Self {
        management::Application {
            metadata: ApplicationMetadata {
                name: app.id,
                labels: app.labels,
                ..Default::default()
            },
            spec: app.data["spec"].as_object().cloned().unwrap_or_default(),
            status: app.data["status"].as_object().cloned().unwrap_or_default(),
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
        Ok(Application {
            id: row.try_get::<_, String>("ID")?,
            data: row.try_get::<_, Json<_>>("DATA")?.0,
            labels: super::labels_to_map(&row)?,
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
        let row = self.client.query_opt(
            "SELECT A2.ID, A2.LABELS, A2.DATA FROM APPLICATION_ALIASES A1 INNER JOIN APPLICATIONS A2 ON A1.ID=A2.ID WHERE A1.ALIAS = $1",
            &[&alias]
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
                "SELECT ID, LABELS, DATA FROM APPLICATIONS WHERE ID = $1",
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

        self.client
            .execute(
                "INSERT INTO APPLICATIONS (ID, LABELS, DATA) VALUES ($1, $2, $3)",
                &[&id, &Json(labels), &Json(data)],
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

        // update device
        let count = self
            .client
            .execute(
                "UPDATE APPLICATIONS SET LABELS = $2, DATA = $3 WHERE ID = $1",
                &[&id, &Json(labels), &Json(data)],
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
