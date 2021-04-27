use actix_web::ResponseError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use drogue_cloud_database_common::{
    auth::authorize, error::ServiceError, models::app::*, DatabaseService,
};
use drogue_cloud_service_api::auth::user::{UserDetails, UserInformation};
use drogue_cloud_service_api::{
    auth::user::authz::{AuthorizationRequest, Outcome},
    health::{HealthCheckError, HealthChecked},
};
use serde::Deserialize;
use tokio_postgres::NoTls;

#[async_trait]
pub trait AuthorizationService: Clone {
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

#[async_trait::async_trait]
impl HealthChecked for PostgresAuthorizationService {
    async fn is_ready(&self) -> Result<(), HealthCheckError> {
        Ok(DatabaseService::is_ready(self)
            .await
            .map_err(HealthCheckError::from)?)
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

impl From<Context> for UserInformation {
    fn from(ctx: Context) -> Self {
        Self::Authenticated(UserDetails {
            user_id: ctx.0.user_id,
            roles: ctx.0.roles,
        })
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

        let outcome = authorize(&application, &Context(request).into());

        log::debug!("Authorization outcome: {:?}", outcome);

        Ok(outcome)
    }
}
