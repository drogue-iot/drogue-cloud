use crate::reqwest::add_service_cert;
use crate::{config::ConfigFromEnv, defaults, openid::ExtendedClaims};
use anyhow::Context;
use core::fmt::{Debug, Formatter};
use failure::Fail;
use futures::{stream, StreamExt, TryStreamExt};
use openid::{
    biscuit::jws::Compact, Claims, Client, CompactJson, Configurable, Discovered, Empty, Jws,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct AuthenticatorConfig {
    #[serde(flatten)]
    pub global: AuthenticatorGlobalConfig,

    #[serde(default)]
    pub oauth: HashMap<String, AuthenticatorClientConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthenticatorGlobalConfig {
    #[serde(default)]
    pub sso_url: Option<String>,

    #[serde(default)]
    pub issuer_url: Option<String>,

    #[serde(default = "defaults::realm")]
    pub realm: String,

    #[serde(default)]
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticatorClientConfig {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,
}

impl ClientConfig for (&AuthenticatorGlobalConfig, &AuthenticatorClientConfig) {
    fn client_id(&self) -> String {
        self.1.client_id.clone()
    }

    fn client_secret(&self) -> String {
        self.1.client_secret.clone()
    }

    fn redirect_url(&self) -> Option<String> {
        self.0.redirect_url.clone()
    }

    fn issuer_url(&self) -> anyhow::Result<Url> {
        let url = self
            .0
            .issuer_url
            .as_ref()
            .cloned()
            .or_else(|| {
                self.0
                    .sso_url
                    .as_ref()
                    .map(|sso| crate::utils::sso_to_issuer_url(&sso, &self.0.realm))
            })
            .ok_or_else(|| anyhow::anyhow!("Missing issuer or SSO URL"))?;

        Url::parse(&url).context("Failed to parse issuer/SSO URL")
    }
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
    clients: Vec<openid::Client<Discovered, ExtendedClaims>>,
}

impl Debug for Authenticator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("Authenticator");
        d.field("client", &"...");
        d.finish()
    }
}

impl From<openid::Client<Discovered, ExtendedClaims>> for Authenticator {
    fn from(client: openid::Client<Discovered, ExtendedClaims>) -> Self {
        Self::from_clients(vec![client])
    }
}

impl Authenticator {
    /// Create a new authenticator by evaluating endpoints and SSO configuration.
    pub async fn new() -> anyhow::Result<Self> {
        Self::from_config(AuthenticatorConfig::from_env()?).await
    }

    pub fn from_clients(clients: Vec<openid::Client<Discovered, ExtendedClaims>>) -> Self {
        Authenticator { clients }
    }

    pub async fn from_config(mut config: AuthenticatorConfig) -> anyhow::Result<Self> {
        let configs = config.oauth.drain().map(|(_, v)| v);
        Self::from_configs(config.global, configs).await
    }

    pub async fn from_configs<I>(
        global: AuthenticatorGlobalConfig,
        configs: I,
    ) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = AuthenticatorClientConfig>,
    {
        let clients = stream::iter(configs)
            .map(Ok)
            .and_then(|config| {
                let global = global.clone();
                async move { create_client(&(&global, &config)).await }
            })
            .try_collect()
            .await?;

        Ok(Self::from_clients(clients))
    }

    fn find_client(
        &self,
        token: &Compact<ExtendedClaims, Empty>,
    ) -> Result<Option<&Client<Discovered, ExtendedClaims>>, AuthenticatorError> {
        let unverified_payload = token.unverified_payload().map_err(|err| {
            log::info!("Failed to decode token payload: {}", err);
            AuthenticatorError::Failed
        })?;

        let client_id = unverified_payload.standard_claims.azp.as_ref();

        log::debug!(
            "Searching client for: {} / {:?}",
            unverified_payload.standard_claims.iss,
            client_id
        );

        // find the client to use

        let client = self.clients.iter().find(|client| {
            let provider_iss = &client.provider.config().issuer;
            let provider_id = &client.client_id;

            log::debug!("Checking client: {} / {}", provider_iss, provider_id);
            if provider_iss != &unverified_payload.standard_claims.iss {
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
    ) -> Result<ExtendedClaims, AuthenticatorError> {
        let mut token: Compact<ExtendedClaims, Empty> = Jws::new_encoded(token.as_ref());

        let client = self.find_client(&token)?.ok_or_else(|| {
            log::debug!("Unable to find client");
            AuthenticatorError::Failed
        })?;

        log::debug!("Using client: {}", client.client_id);

        //std::panic::catch_unwind(|| {
        // Decode_token may panic if an unsupported algorithm is used. As that maybe user input,
        // that could mean that a user could trigger a panic in this code. However, to us
        // an unsupported algorithm simply means we reject the authentication.
        client.decode_token(&mut token).map_err(|err| {
                log::debug!("Failed to decode token: {}", err);
                AuthenticatorError::Failed
            })?
        //})
        //.map_err(|_| AuthenticatorError::Failed)??; // double '?' yes, 'catch_unwind' returns Result<Result<_>> here
        ;

        log::debug!("Token: {:#?}", token);

        client.validate_token(&token, None, None).map_err(|err| {
            log::info!("Validation failed: {}", err);
            AuthenticatorError::Failed
        })?;

        match token {
            Compact::Decoded { payload, .. } => Ok(payload),
            Compact::Encoded(_) => Err(AuthenticatorError::Failed),
        }
    }
}

pub trait ClientConfig {
    fn client_id(&self) -> String;
    fn client_secret(&self) -> String;
    fn redirect_url(&self) -> Option<String>;
    fn issuer_url(&self) -> anyhow::Result<Url>;
}

pub async fn create_client<C: ClientConfig, P: CompactJson + Claims>(
    config: &C,
) -> anyhow::Result<openid::Client<Discovered, P>> {
    let mut client = reqwest::ClientBuilder::new();

    client = add_service_cert(client)?;

    let client = openid::Client::<Discovered, P>::discover_with_client(
        client.build()?,
        config.client_id(),
        config.client_secret(),
        config.redirect_url(),
        config.issuer_url()?,
    )
    .await
    .map_err(|err| anyhow::Error::from(err.compat()))?;

    log::info!("Discovered OpenID: {:#?}", client.config());

    Ok(client)
}

#[cfg(test)]
mod test {

    use super::*;
    use openid::biscuit::ClaimsSet;

    #[test]
    fn test_decode() -> anyhow::Result<()> {
        let token = r#"eyJhbGciOiJSUzI1NiIsInR5cCIgOiAiSldUIiwia2lkIiA6ICJEZ2hoSVVwV2llSU5jX0Jtc0lDckhHbm1WTDNMMTMteURtVmp3N2MwUnlFIn0.eyJleHAiOjE2MTg0OTQ5MjYsImlhdCI6MTYxODQ5NDYyNiwianRpIjoiNjAzYTNhMGYtZTkzMC00ZjE1LTkwMDUtMTZjNzFiMTllNDdiIiwiaXNzIjoiaHR0cHM6Ly9rZXljbG9hay1kcm9ndWUtZGV2LmFwcHMud29uZGVyZnVsLmlvdC1wbGF5Z3JvdW5kLm9yZy9hdXRoL3JlYWxtcy9kcm9ndWUiLCJhdWQiOlsic2VydmljZXMiLCJncmFmYW5hIiwiZGl0dG8iLCJkcm9ndWUiLCJhY2NvdW50Il0sInN1YiI6ImI4ZWZjZjAwLTJmZmYtNDRlYS1hZGU5LWYzNWViMmY0ZmNlMSIsInR5cCI6IkJlYXJlciIsImF6cCI6InNlcnZpY2VzIiwiYWNyIjoiMSIsInJlYWxtX2FjY2VzcyI6eyJyb2xlcyI6WyJvZmZsaW5lX2FjY2VzcyIsInVtYV9hdXRob3JpemF0aW9uIl19LCJyZXNvdXJjZV9hY2Nlc3MiOnsiZ3JhZmFuYSI6eyJyb2xlcyI6WyJncmFmYW5hLWVkaXRvciIsImdyYWZhbmEtYWRtaW4iXX0sImRpdHRvIjp7InJvbGVzIjpbImRpdHRvLXVzZXIiLCJkaXR0by1hZG1pbiJdfSwiZHJvZ3VlIjp7InJvbGVzIjpbImRyb2d1ZS11c2VyIiwiZHJvZ3VlLWFkbWluIl19LCJzZXJ2aWNlcyI6eyJyb2xlcyI6WyJkcm9ndWUtdXNlciIsImRyb2d1ZS1hZG1pbiJdfSwiYWNjb3VudCI6eyJyb2xlcyI6WyJtYW5hZ2UtYWNjb3VudCIsIm1hbmFnZS1hY2NvdW50LWxpbmtzIiwidmlldy1wcm9maWxlIl19fSwic2NvcGUiOiJlbWFpbCBwcm9maWxlIiwiY2xpZW50SWQiOiJzZXJ2aWNlcyIsImVtYWlsX3ZlcmlmaWVkIjpmYWxzZSwiY2xpZW50SG9zdCI6IjE5Mi4xNjguMTIuMSIsInByZWZlcnJlZF91c2VybmFtZSI6InNlcnZpY2UtYWNjb3VudC1zZXJ2aWNlcyIsImNsaWVudEFkZHJlc3MiOiIxOTIuMTY4LjEyLjEifQ.JNvytxz-IqTXXoUKF8xZMw-diS7jtkz9GP4u6MRo9iny410zTxSl5Z_O9Mhy1LofxPBMYt65JWs6tRBdKAEXa0w5bLbZdyRgdr3SJpDAxIz6CezCHqSDl1OSQPrW_rWmaS_9XLWxl8fgADwLCNjWbrZrsls_E_rDdfjqhrvcE4f2__lIV_oeG7zcfyYJzNVoZ3Ukyadxq6fwAMf8kZwU_6R6hClb0Ya6jLpNE3miy3ZgugZ1QLJT3tSTyyxzSHMy8146ncBughepequ-zKSnbzQjhgwQsARjjv7bBeZgRjRY6kF3Wr8JalaR2DZU49RopfegZ-9PWO2AEH2dxe4OfQ"#;
        let token: Compact<ClaimsSet<serde_json::Value>, serde_json::Value> =
            Jws::new_encoded(token);

        println!("Header: {:#?}", token.unverified_header());
        println!("Payload: {:#?}", token.unverified_payload());

        let token = match token {
            Compact::Encoded(encoded) => {
                let header = encoded.part(0)?;
                let decoded_claims = encoded.part(1)?;
                Jws::new_decoded(header, decoded_claims)
            }
            Compact::Decoded { .. } => token,
        };

        println!("Token: {:#?}", token);

        Ok(())
    }
}
