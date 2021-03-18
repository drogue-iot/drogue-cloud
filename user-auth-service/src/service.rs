use actix_web::ResponseError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use drogue_cloud_database_common::auth::authorize;
use drogue_cloud_database_common::{error::ServiceError, models::app::*, DatabaseService};
use drogue_cloud_service_api::{
    auth::authz::{AuthorizationRequest, Outcome},
    health::HealthCheckedService,
};
use drogue_cloud_service_common::auth::Identity;
use serde::Deserialize;
use tokio_postgres::NoTls;

#[async_trait]
pub trait AuthorizationService: HealthCheckedService + Clone {
    type Error: ResponseError;

    async fn authorize(&self, request: AuthorizationRequest) -> Result<Outcome, Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthorizationServiceConfig {
    pub pg: deadpool_postgres::Config,
}

impl DatabaseService for PostgresAuthorizationService {
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait]
impl HealthCheckedService for PostgresAuthorizationService {
    type HealthCheckError = ServiceError;

    async fn is_ready(&self) -> Result<(), Self::HealthCheckError> {
        (self as &dyn DatabaseService).is_ready().await
    }
}

#[derive(Clone)]
pub struct PostgresAuthorizationService {
    pool: Pool,
}

impl PostgresAuthorizationService {
    pub fn new(config: AuthorizationServiceConfig) -> anyhow::Result<Self> {
        Ok(Self {
            pool: config.pg.create_pool(NoTls)?,
        })
    }
}

struct Context(pub AuthorizationRequest);

impl Identity for Context {
    fn user_id(&self) -> Option<&str> {
        Some(self.0.user_id.as_str())
    }
}

#[async_trait]
impl AuthorizationService for PostgresAuthorizationService {
    type Error = ServiceError;

    async fn authorize(&self, request: AuthorizationRequest) -> Result<Outcome, Self::Error> {
        let c = self.pool.get().await?;

        // lookup the application

        let application = PostgresApplicationAccessor::new(&c);
        let application = match application.lookup(&request.application).await? {
            Some(application) => application,
            None => {
                return Ok(Outcome::Deny);
            }
        };

        log::debug!("Found application: {:?}", application.name);

        let outcome = authorize(&application, &Context(request));

        log::debug!("Authorization outcome: {:?}", outcome);

        Ok(outcome)
    }
}
