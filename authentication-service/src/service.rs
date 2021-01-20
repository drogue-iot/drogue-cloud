use crate::db::{device::*, tenant::*};
use actix_web::{HttpResponse, ResponseError};
use async_trait::async_trait;
use deadpool_postgres::{Pool, PoolError};
use drogue_cloud_service_api::{AuthenticationRequest, Credential, Device, Outcome, Tenant};
use drogue_cloud_service_common::error::ErrorResponse;
use serde::Deserialize;
use thiserror::Error;
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

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Pool error: {0}")]
    Pool(#[from] PoolError),
    #[error("Database error: {0}")]
    Database(#[from] tokio_postgres::Error),
}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::Internal(message) => {
                HttpResponse::InternalServerError().json(ErrorResponse {
                    error: "InternalError".into(),
                    message: message.clone(),
                })
            }
            ServiceError::Pool(cause) => HttpResponse::ServiceUnavailable().json(ErrorResponse {
                error: "PoolError".into(),
                message: format!("{}", cause),
            }),
            ServiceError::Database(cause) => {
                HttpResponse::ServiceUnavailable().json(ErrorResponse {
                    error: "DatabaseError".into(),
                    message: format!("{}", cause),
                })
            }
        }
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
            None => {
                return Ok(Outcome::Fail);
            }
            Some(tenant) => tenant,
        };

        // lookup the device

        let device = PostgresDeviceAccessor::new(&c);
        let device = match device.lookup(&tenant.id, &request.device).await? {
            None => {
                return Ok(Outcome::Fail);
            }
            Some(device) => device,
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

fn validate_credential(_: &Tenant, device: &Device, cred: &Credential) -> bool {
    match cred {
        Credential::Password(provided_password) => {
            device.data.credentials.iter().any(|c| match c {
                // match passwords
                Credential::Password(stored_password) => stored_password == provided_password,
                // match passwords if the stored username is equal to the device id
                Credential::UsernamePassword {
                    username: stored_username,
                    password: stored_password,
                } if stored_username == &device.id => stored_password == provided_password,
                // no match
                _ => false,
            })
        }
        Credential::UsernamePassword {
            username: provided_username,
            password: provided_password,
        } => device.data.credentials.iter().any(|c| match c {
            // match passwords if the provided username is equal to the device id
            Credential::Password(stored_password) if provided_username == &device.id => {
                stored_password == provided_password
            }
            // match username/password against username/password
            Credential::UsernamePassword {
                username: stored_username,
                password: stored_password,
            } => stored_username == provided_username && stored_password == provided_password,
            // no match
            _ => false,
        }),
        _ => false,
    }
}
