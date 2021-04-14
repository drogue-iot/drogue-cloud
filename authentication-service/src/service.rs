use actix_web::ResponseError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use drogue_client::{registry, Dialect, Translator};
use drogue_cloud_database_common::{
    error::ServiceError,
    models::{app::*, device::*},
    DatabaseService,
};
use drogue_cloud_service_api::{
    auth::authn::{self, AuthenticationRequest, Outcome},
    health::{HealthCheckError, HealthChecked},
};
use rustls::{AllowAnyAuthenticatedClient, Certificate, RootCertStore};
use serde::Deserialize;
use std::io::Cursor;
use tokio_postgres::NoTls;

#[async_trait]
pub trait AuthenticationService: Clone {
    type Error: ResponseError;

    async fn authenticate(&self, request: AuthenticationRequest) -> Result<Outcome, Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthenticationServiceConfig {
    pub pg: deadpool_postgres::Config,
}

impl DatabaseService for PostgresAuthenticationService {
    fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait::async_trait]
impl HealthChecked for PostgresAuthenticationService {
    async fn is_ready(&self) -> Result<(), HealthCheckError> {
        Ok(DatabaseService::is_ready(self)
            .await
            .map_err(HealthCheckError::from)?)
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

macro_rules! pass {
    ($application:expr, $device:expr, $as_device:expr) => {{
        log::info!("app {:?}", $application);
        Outcome::Pass {
            application: $application,
            device: strip_credentials($device),
            r#as: $as_device.map(strip_credentials),
        }
    }};
}

#[async_trait]
impl AuthenticationService for PostgresAuthenticationService {
    type Error = ServiceError;

    async fn authenticate(&self, request: AuthenticationRequest) -> Result<Outcome, Self::Error> {
        let c = self.pool.get().await?;

        // lookup the application

        let application = PostgresApplicationAccessor::new(&c);
        let application = match application.lookup(&request.application).await? {
            Some(application) => application.into(),
            None => {
                return Ok(Outcome::Fail);
            }
        };

        log::debug!("Found application: {:?}", application);

        // validate application

        if !validate_app(&application) {
            return Ok(Outcome::Fail);
        }

        // lookup the device

        let accessor = PostgresDeviceAccessor::new(&c);
        let device = match accessor
            .lookup(&application.metadata.name, &request.device)
            .await?
        {
            Some(device) => device.into(),
            None => {
                return Ok(Outcome::Fail);
            }
        };

        log::debug!("Found device: {:?}", device);

        // validate credential

        Ok(
            match validate_credential(&application, &device, request.credential) {
                true => {
                    // check gateway
                    match request.r#as {
                        Some(as_id) => {
                            if as_id != request.device {
                                match accessor.lookup(&application.metadata.name, &as_id).await? {
                                    Some(as_device) => {
                                        let as_manage: registry::v1::Device = as_device.into();
                                        match as_manage
                                            .section::<registry::v1::DeviceSpecGatewaySelector>()
                                        {
                                            Some(Ok(gateway_selector)) => {
                                                if gateway_selector
                                                    .match_names
                                                    .contains(&request.device)
                                                {
                                                    log::debug!(
                                                        "Device {:?} allowed to publish as {:?}",
                                                        &request.device,
                                                        as_id
                                                    );
                                                    pass!(application, device, Some(as_manage))
                                                } else {
                                                    log::debug!("Device {:?} not allowed to publish as {:?}, gateway not listed", &request.device, as_id);
                                                    Outcome::Fail
                                                }
                                            }
                                            _ => {
                                                log::debug!("Device {:?} not allowed to publish as {:?}, no gateways configured", &request.device, as_id);
                                                Outcome::Fail
                                            }
                                        }
                                    }
                                    None => {
                                        log::debug!("Device {:?} not allowed to publish as {:?}, device does not exist", &request.device, as_id);
                                        Outcome::Fail
                                    }
                                }
                            } else {
                                pass!(application, device, None)
                            }
                        }
                        None => {
                            pass!(application, device, None)
                        }
                    }
                }
                false => Outcome::Fail,
            },
        )
    }
}

/// Strip the credentials from the device information, so that we do not leak them.
fn strip_credentials(mut device: registry::v1::Device) -> registry::v1::Device {
    // FIXME: we need to do a better job here, maybe add a "secrets" section instead
    device
        .spec
        .remove(registry::v1::DeviceSpecCredentials::key());
    device
}

/// Validate if an application is "ok" to be used for authentication.
fn validate_app(app: &registry::v1::Application) -> bool {
    match app.section::<registry::v1::DeviceSpecCore>() {
        // found "core", decoded successfully -> check
        Some(Ok(core)) => {
            if core.disabled {
                return false;
            }
        }
        // found "core", but could not decode -> fail
        Some(Err(_)) => {
            return false;
        }
        // no "core" section
        _ => {}
    };

    // done
    true
}

fn validate_credential(
    app: &registry::v1::Application,
    device: &registry::v1::Device,
    cred: authn::Credential,
) -> bool {
    let credentials = match device.section::<registry::v1::DeviceSpecCredentials>() {
        Some(Ok(credentials)) => credentials.credentials,
        _ => {
            log::debug!("Missing or invalid device credentials section");
            return false;
        }
    };

    log::debug!("Checking credentials: {:?}", cred);

    match cred {
        authn::Credential::Password(provided_password) => {
            validate_password(device, &credentials, &provided_password)
        }
        authn::Credential::UsernamePassword {
            username: provided_username,
            password: provided_password,
            ..
        } => {
            validate_username_password(device, &credentials, &provided_username, &provided_password)
        }
        authn::Credential::Certificate(chain) => {
            let now = Utc::now();
            validate_certificate(app, device, &credentials, chain, &now)
        }
    }
}

/// validate if a provided password matches
fn validate_password(
    device: &registry::v1::Device,
    credentials: &[registry::v1::Credential],
    provided_password: &str,
) -> bool {
    credentials.iter().any(|c| match c {
        // match passwords
        registry::v1::Credential::Password(stored_password) => stored_password == provided_password,
        // match passwords if the stored username is equal to the device id
        registry::v1::Credential::UsernamePassword {
            username: stored_username,
            password: stored_password,
            ..
        } if stored_username == &device.metadata.name => stored_password == provided_password,
        // no match
        _ => false,
    })
}

/// validate if a provided username/password combination matches
fn validate_username_password(
    device: &registry::v1::Device,
    credentials: &[registry::v1::Credential],
    provided_username: &str,
    provided_password: &str,
) -> bool {
    credentials.iter().any(|c| match c {
        // match passwords if the provided username is equal to the device id
        registry::v1::Credential::Password(stored_password)
            if provided_username == device.metadata.name =>
        {
            stored_password == provided_password
        }
        // match username/password against username/password
        registry::v1::Credential::UsernamePassword {
            username: stored_username,
            password: stored_password,
            ..
        } => stored_username == provided_username && stored_password == provided_password,
        // no match
        _ => false,
    })
}

/// validate if a provided certificate chain matches
fn validate_certificate(
    app: &registry::v1::Application,
    _device: &registry::v1::Device,
    _credentials: &[registry::v1::Credential],
    provided_chain: Vec<Vec<u8>>,
    now: &DateTime<Utc>,
) -> bool {
    if provided_chain.is_empty() {
        return false;
    }

    if let Some(Ok(anchors)) = app.section::<registry::v1::ApplicationStatusTrustAnchors>() {
        // if we have some trust anchors
        let mut presented_certs = Vec::with_capacity(provided_chain.len());
        for cert in provided_chain {
            presented_certs.push(Certificate(cert));
        }

        // test them
        anchors
            .anchors
            .iter()
            .any(|a| validate_trust_anchor(a, now, &presented_certs))
    } else {
        false
    }
}

/// validate if a provided certificate chain matches the trust anchor to test
fn validate_trust_anchor(
    anchor: &registry::v1::ApplicationStatusTrustAnchorEntry,
    now: &DateTime<Utc>,
    presented_certs: &[Certificate],
) -> bool {
    if let registry::v1::ApplicationStatusTrustAnchorEntry::Valid {
        subject: _,
        certificate,
        not_before,
        not_after,
    } = anchor
    {
        // quick validity period check before actually checking the chain
        if now < not_before {
            return false;
        }
        if now > not_after {
            return false;
        }

        // create root from trust anchor entry
        let mut roots = RootCertStore::empty();
        let mut c = Cursor::new(certificate);
        if roots.add_pem_file(&mut c).is_err() {
            log::debug!("Failed to parse certificates");
            return false;
        }

        let v = AllowAnyAuthenticatedClient::new(roots);
        match v.verify_client_cert(presented_certs, None) {
            Ok(_) => true,
            Err(err) => {
                log::debug!("Failed to verify client certificate: {}", err);
                false
            }
        }
    } else {
        false
    }
}
