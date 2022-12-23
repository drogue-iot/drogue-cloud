use actix_web::ResponseError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use drogue_client::user::v1::authz::{AuthorizationRequest, Outcome};
use drogue_cloud_database_common::{
    auth::authorize,
    error::ServiceError,
    models::{app::*, Lock},
    postgres, DatabaseService,
};
use drogue_cloud_service_api::{
    auth::user::{UserDetails, UserInformation},
    health::{HealthCheckError, HealthChecked},
    webapp as actix_web,
};
use serde::Deserialize;

#[async_trait]
pub trait AuthorizationService: Clone {
    type Error: ResponseError;

    async fn authorize(&self, request: AuthorizationRequest) -> Result<Outcome, Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthorizationServiceConfig {
    pub pg: postgres::Config,
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
            pool: config.pg.create_pool()?,
        })
    }
}

struct Context(pub AuthorizationRequest);

impl From<Context> for UserInformation {
    fn from(ctx: Context) -> Self {
        match ctx.0.user_id {
            Some(user_id) if !user_id.is_empty() => Self::Authenticated(UserDetails {
                user_id,
                roles: ctx.0.roles,
                // fixme
                claims: None,
            }),
            _ => Self::Anonymous,
        }
    }
}

#[async_trait]
impl AuthorizationService for PostgresAuthorizationService {
    type Error = ServiceError;

    async fn authorize(&self, request: AuthorizationRequest) -> Result<Outcome, Self::Error> {
        let c = self.pool.get().await?;

        // lookup the application

        let application = PostgresApplicationAccessor::new(&c);
        let application = match application.get(&request.application, Lock::None).await? {
            Some(application) => application,
            None => {
                return Ok(Outcome::Deny);
            }
        };

        log::debug!(
            "Found application: {:?} - members: {:?}",
            application.name,
            application.members
        );
        log::debug!(
            "User - ID: {:?}, roles: {:?}",
            request.user_id,
            request.roles
        );

        let permission = request.permission;
        let outcome = authorize(&application, &Context(request).into(), permission);

        log::debug!("Authorization outcome: {:?} -> {:?}", permission, outcome);

        Ok(outcome)
    }
}
