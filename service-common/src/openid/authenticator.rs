use crate::{config::ConfigFromEnv, defaults, endpoints::eval_endpoints};
use anyhow::Context;
use core::fmt::{Debug, Formatter};
use drogue_cloud_service_api::endpoints::Endpoints;
use failure::Fail;
use futures::{stream, StreamExt, TryStreamExt};
use openid::{biscuit::jws::Compact, Client, Configurable, Empty, Jws, StandardClaims};
use reqwest::Certificate;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::File, io::Read, path::Path};
use thiserror::Error;
use url::Url;

const SERVICE_CA_CERT: &str = "/var/run/secrets/kubernetes.io/serviceaccount/service-ca.crt";

#[derive(Clone, Debug, Deserialize)]
pub struct AuthenticatorConfig {
    #[serde(default)]
    pub oauth: HashMap<String, AuthenticatorClientConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticatorClientConfig {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,
}

#[derive(Debug, Error)]
pub enum AuthenticatorError {
    #[error("Missing authenticator instance")]
    Missing,
    #[error("Authentication failed")]
    Failed,
}

/// An authenticator to authenticate incoming requests.
#[derive(Clone)]
pub struct Authenticator {
    clients: Vec<openid::Client>,
}

impl Debug for Authenticator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("Authenticator");
        d.field("client", &"...");
        d.finish()
    }
}

impl From<openid::Client> for Authenticator {
    fn from(client: Client) -> Self {
        Self::from_clients(vec![client])
    }
}

impl Authenticator {
    /// Create a new authenticator by evaluating endpoints and SSO configuration.
    pub async fn new() -> anyhow::Result<Self> {
        Self::from_endpoints(eval_endpoints().await?).await
    }

    pub async fn from_endpoints(endpoints: Endpoints) -> anyhow::Result<Self> {
        let config = AuthenticatorConfig::from_env()?;
        Self::from_config(config, endpoints).await
    }

    pub fn from_clients(clients: Vec<openid::Client>) -> Self {
        Authenticator { clients }
    }

    pub async fn from_config(
        mut config: AuthenticatorConfig,
        endpoints: Endpoints,
    ) -> anyhow::Result<Self> {
        let configs = config.oauth.drain().map(|(_, v)| v);
        Self::from_configs(configs, endpoints).await
    }

    pub async fn from_configs<I>(configs: I, endpoints: Endpoints) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = AuthenticatorClientConfig>,
    {
        let clients = stream::iter(configs)
            .map(Ok)
            .and_then(|config| async { create_client(config, endpoints.clone()).await })
            .try_collect()
            .await?;

        Ok(Self::from_clients(clients))
    }

    fn find_client(
        &self,
        token: &Compact<StandardClaims, Empty>,
    ) -> Result<Option<&Client>, AuthenticatorError> {
        let unverified_payload = token.unverified_payload().map_err(|err| {
            log::info!("Failed to decode token payload: {}", err);
            AuthenticatorError::Failed
        })?;

        let client_id = unverified_payload.azp.as_ref();

        log::debug!(
            "Searching client for: {} / {:?}",
            unverified_payload.iss,
            client_id
        );

        // find the client to use

        let client = self.clients.iter().find(|client| {
            let provider_iss = &client.provider.config().issuer;
            let provider_id = &client.client_id;

            log::debug!("Checking client: {} / {}", provider_iss, provider_id);
            if provider_iss != &unverified_payload.iss {
                return false;
            }
            if let Some(client_id) = client_id {
                if client_id != provider_id {
                    return false;
                }
            }

            true
        });

        Ok(client)
    }

    /// Validate a bearer token.
    pub async fn validate_token<S: AsRef<str>>(
        &self,
        token: S,
    ) -> Result<Compact<StandardClaims, Empty>, AuthenticatorError> {
        let mut token = Jws::new_encoded(token.as_ref());

        let client = self.find_client(&token)?.ok_or_else(|| {
            log::debug!("Unable to find client");
            AuthenticatorError::Failed
        })?;

        log::debug!("Using client: {}", client.client_id);

        client.decode_token(&mut token).map_err(|err| {
            log::debug!("Failed to decode token: {}", err);
            AuthenticatorError::Failed
        })?;

        log::debug!("Token: {:#?}", token);

        client.validate_token(&token, None, None).map_err(|err| {
            log::info!("Validation failed: {}", err);
            AuthenticatorError::Failed
        })?;

        Ok(token)
    }
}

impl ClientConfig for AuthenticatorClientConfig {
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

pub async fn create_client<C: ClientConfig>(
    config: C,
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
            "Failed to detect 'issuer URL'. Consider using an env-var based configuration."
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
