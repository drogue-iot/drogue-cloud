use actix_web::ResponseError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use drogue_cloud_database_common::{
    error::ServiceError,
    models::{device::*, tenant::*},
};
use drogue_cloud_service_api::{
    auth::{self, AuthenticationRequest, Outcome},
    management::{self, Device, Tenant},
};
use serde::Deserialize;
use tokio_postgres::NoTls;

#[async_trait]
pub trait AuthenticationService: Clone {
    type Error: ResponseError;

    async fn authenticate(&self, request: AuthenticationRequest) -> Result<Outcome, Self::Error>;
    async fn is_ready(&self) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthenticationServiceConfig {
    pub pg: deadpool_postgres::Config,
}

impl AuthenticationServiceConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let mut cfg = config::Config::new();
        cfg.merge(config::Environment::new().separator("__"))?;
        cfg.try_into()
    }
}

#[derive(Clone)]
pub struct PostgresAuthenticationService {
    pool: Pool,
}

impl PostgresAuthenticationService {
    pub fn new(config: AuthenticationServiceConfig) -> anyhow::Result<Self> {
        Ok(Self {
            pool: config.pg.create_pool(NoTls)?,
        })
    }
}

#[async_trait]
impl AuthenticationService for PostgresAuthenticationService {
    type Error = ServiceError;

    async fn authenticate(&self, request: AuthenticationRequest) -> Result<Outcome, Self::Error> {
        let c = self.pool.get().await?;

        // lookup the tenant

        let tenant = PostgresTenantAccessor::new(&c);
        let tenant = match tenant.lookup(&request.tenant).await? {
            Some(tenant) => tenant,
            None => {
                return Ok(Outcome::Fail);
            }
        };

        // validate tenant

        if !validate_tenant(&tenant) {
            return Ok(Outcome::Fail);
        }

        // lookup the device

        let device = PostgresDeviceAccessor::new(&c);
        let device = match device.lookup(&tenant.id, &request.device).await? {
            Some(device) => device,
            None => {
                return Ok(Outcome::Fail);
            }
        };

        // validate credential

        Ok(
            match validate_credential(&tenant, &device, &request.credential) {
                true => Outcome::Pass {
                    tenant,
                    device: strip_credentials(device),
                },
                false => Outcome::Fail,
            },
        )
    }

    async fn is_ready(&self) -> Result<(), Self::Error> {
        self.pool.get().await?.simple_query("SELECT 1").await?;
        Ok(())
    }
}

fn strip_credentials(mut device: Device) -> Device {
    device.data.credentials.clear();
    device
}

/// Validate if a tenant is "ok" to be used for authentication.
fn validate_tenant(tenant: &Tenant) -> bool {
    // validate "disabled"
    if tenant.data.disabled {
        return false;
    }

    // done
    true
}

fn validate_credential(_: &Tenant, device: &Device, cred: &auth::Credential) -> bool {
    match cred {
        auth::Credential::Password(provided_password) => {
            device.data.credentials.iter().any(|c| match c {
                // match passwords
                management::Credential::Password(stored_password) => {
                    stored_password == provided_password
                }
                // match passwords if the stored username is equal to the device id
                management::Credential::UsernamePassword {
                    username: stored_username,
                    password: stored_password,
                    ..
                } if stored_username == &device.id => stored_password == provided_password,
                // no match
                _ => false,
            })
        }
        auth::Credential::UsernamePassword {
            username: provided_username,
            password: provided_password,
            ..
        } => device.data.credentials.iter().any(|c| match c {
            // match passwords if the provided username is equal to the device id
            management::Credential::Password(stored_password)
                if provided_username == &device.id =>
            {
                stored_password == provided_password
            }
            // match username/password against username/password
            management::Credential::UsernamePassword {
                username: stored_username,
                password: stored_password,
                ..
            } => stored_username == provided_username && stored_password == provided_password,
            // no match
            _ => false,
        }),
        _ => false,
    }
}
