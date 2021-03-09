use anyhow::Context;
use core::fmt::Formatter;
use drogue_cloud_service_api::endpoints::Endpoints;
use envconfig::Envconfig;
use failure::Fail;
use openid::Jws;
use reqwest::Certificate;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use thiserror::Error;
use url::Url;

const SERVICE_CA_CERT: &str = "/var/run/secrets/kubernetes.io/serviceaccount/service-ca.crt";

#[derive(Debug, Envconfig)]
pub struct AuthConfig {
    #[envconfig(from = "CLIENT_ID")]
    pub client_id: String,
    #[envconfig(from = "CLIENT_SECRET")]
    pub client_secret: String,
    // Note: "roles" may be required for the "aud" claim when using Keycloak
    #[envconfig(from = "SCOPES", default = "openid profile email")]
    pub scopes: String,
}

#[derive(Debug, Error)]
pub enum AuthenticatorError {
    #[error("Missing authenticator instance")]
    Missing,
    #[error("Authentication failed")]
    Failed,
}

pub struct Authenticator {
    pub client: Option<openid::Client>,
    pub scopes: String,
}

impl Debug for Authenticator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("Authenticator");

        match self.client {
            None => {
                d.field("client", &"None".to_string());
            }
            Some(_) => {
                d.field("client", &"Some(...)".to_string());
            }
        }

        d.finish()
    }
}

impl Authenticator {
    pub async fn validate_token<S: AsRef<str>>(&self, token: S) -> Result<(), AuthenticatorError> {
        let client = self.client.as_ref().ok_or(AuthenticatorError::Missing)?;

        let mut token = Jws::new_encoded(token.as_ref());
        match client.decode_token(&mut token) {
            Ok(_) => Ok(()),
            Err(err) => {
                log::info!("Failed to decode token: {}", err);
                Err(AuthenticatorError::Failed)
            }
        }?;

        log::info!("Token: {:#?}", token);

        match client.validate_token(&token, None, None) {
            Ok(_) => Ok(()),
            Err(err) => {
                log::info!("Validation failed: {}", err);
                Err(AuthenticatorError::Failed)
            }
        }
    }
}

impl ClientConfig for AuthConfig {
    fn client_id(&self) -> String {
        self.client_id.clone()
    }

    fn client_secret(&self) -> String {
        self.client_secret.clone()
    }
}

pub trait ClientConfig {
    fn client_id(&self) -> String;
    fn client_secret(&self) -> String;
}

pub async fn create_client(
    config: &dyn ClientConfig,
    endpoints: Endpoints,
) -> anyhow::Result<openid::Client> {
    let mut client = reqwest::ClientBuilder::new();

    client = add_service_cert(client)?;

    let Endpoints {
        redirect_url,
        issuer_url,
        ..
    } = endpoints;

    let issuer_url = issuer_url.ok_or_else(|| {
        anyhow::anyhow!(
            "Failed to detect 'issuer URL'. Consider using a env-var based configuration."
        )
    })?;

    let client = openid::DiscoveredClient::discover_with_client(
        client.build()?,
        config.client_id(),
        config.client_secret(),
        redirect_url,
        Url::parse(&issuer_url)
            .with_context(|| format!("Failed to parse issuer URL: {}", issuer_url))?,
    )
    .await
    .map_err(|err| anyhow::Error::from(err.compat()))?;

    log::info!("Discovered OpenID: {:#?}", client.config());

    Ok(client)
}

fn add_service_cert(mut client: reqwest::ClientBuilder) -> anyhow::Result<reqwest::ClientBuilder> {
    let cert = Path::new(SERVICE_CA_CERT);
    if cert.exists() {
        log::info!("Adding root certificate: {}", SERVICE_CA_CERT);
        let mut file = File::open(cert)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let pems = pem::parse_many(buf);
        let pems = pems
            .into_iter()
            .map(|pem| {
                Certificate::from_pem(&pem::encode(&pem).into_bytes()).map_err(|err| err.into())
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        log::info!("Found {} certificates", pems.len());

        for pem in pems {
            log::info!("Adding root certificate: {:?}", pem);
            client = client.add_root_certificate(pem);
        }
    } else {
        log::info!(
            "Service CA certificate does not exist, skipping! ({})",
            SERVICE_CA_CERT
        );
    }

    Ok(client)
}

pub trait Expires {
    /// Check if the resources expires before the duration elapsed.
    fn expires_before(&self, duration: chrono::Duration) -> bool {
        match self.expires_in() {
            Some(expires) => expires >= duration,
            None => false,
        }
    }

    /// Get the duration until this resource expires. This may be negative.
    fn expires_in(&self) -> Option<chrono::Duration> {
        self.expires().map(|expires| expires - chrono::Utc::now())
    }

    /// Get the timestamp when the resource expires.
    fn expires(&self) -> Option<chrono::DateTime<chrono::Utc>>;
}

impl Expires for openid::Bearer {
    fn expires(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.expires
    }
}
