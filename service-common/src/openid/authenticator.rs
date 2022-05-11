use crate::{
    defaults,
    openid::{CommaSeparatedVec, ExtendedClaims},
    reqwest::ClientFactory,
};
use anyhow::Context;
use core::fmt::{Debug, Formatter};
use futures::{stream, StreamExt, TryStreamExt};
use openid::{
    biscuit::jws::Compact, Claims, Client, CompactJson, Configurable, Discovered, Empty, Jws,
};
use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;
use tracing::instrument;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct AuthenticatorConfig {
    #[serde(default)]
    pub disabled: bool,

    #[serde(flatten)]
    pub global: AuthenticatorGlobalConfig,

    #[serde(default)]
    pub clients: HashMap<String, AuthenticatorClientConfig>,
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

    #[serde(default)]
    pub tls_insecure: bool,

    #[serde(default)]
    pub tls_ca_certificates: CommaSeparatedVec,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AuthenticatorClientConfig {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,
    #[serde(default)]
    pub issuer_url: Option<String>,

    #[serde(default)]
    pub tls_insecure: Option<bool>,
    #[serde(default)]
    pub tls_ca_certificates: Option<CommaSeparatedVec>,
}

impl AuthenticatorConfig {
    /// Create a client from a configuration. This respects the "disabled" field and returns
    /// `None` in this case.
    pub async fn into_client(self) -> anyhow::Result<Option<Authenticator>> {
        if self.disabled {
            Ok(None)
        } else {
            Ok(Some(Authenticator::new(self).await?))
        }
    }
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
            .1
            .issuer_url
            .clone()
            .or_else(|| self.0.issuer_url.clone())
            .or_else(|| {
                self.0
                    .sso_url
                    .as_ref()
                    .map(|sso| crate::utils::sso_to_issuer_url(sso, &self.0.realm))
            })
            .ok_or_else(|| anyhow::anyhow!("Missing issuer or SSO URL"))?;

        Url::parse(&url).context("Failed to parse issuer/SSO URL")
    }

    fn tls_insecure(&self) -> bool {
        self.1.tls_insecure.unwrap_or(self.0.tls_insecure)
    }

    fn tls_ca_certificates(&self) -> Vec<String> {
        self.1
            .tls_ca_certificates
            .clone()
            .unwrap_or_else(|| self.0.tls_ca_certificates.clone())
            .0
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

struct ClientsDebug<'a>(&'a [openid::Client<Discovered, ExtendedClaims>]);
impl<'a> Debug for ClientsDebug<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_list();
        for c in self.0 {
            d.entry(&c.client_id);
        }
        d.finish()
    }
}

impl Debug for Authenticator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("Authenticator");
        d.field("clients", &ClientsDebug(self.clients.as_slice()));
        d.finish()
    }
}

impl From<openid::Client<Discovered, ExtendedClaims>> for Authenticator {
    fn from(client: openid::Client<Discovered, ExtendedClaims>) -> Self {
        Self::from_clients(vec![client])
    }
}

impl Authenticator {
    pub fn from_clients(clients: Vec<openid::Client<Discovered, ExtendedClaims>>) -> Self {
        Authenticator { clients }
    }

    /// Create a new authenticator by evaluating endpoints and SSO configuration.
    pub async fn new(mut config: AuthenticatorConfig) -> anyhow::Result<Self> {
        let configs = config.clients.drain().map(|(_, v)| v);
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
    #[instrument(level = "debug", skip_all, fields(token=token.as_ref()), ret)]
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

        // decode_token may panic if an unsupported algorithm is used. As that maybe user input,
        // that could mean that a user could trigger a panic in this code. However, to us
        // an unsupported algorithm simply means we reject the authentication.
        client.decode_token(&mut token).map_err(|err| {
            log::debug!("Failed to decode token: {}", err);
            AuthenticatorError::Failed
        })?;

        log::debug!("Token: {:?}", token);

        super::validate::validate_token(client, &token, None).map_err(|err| {
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
    fn tls_insecure(&self) -> bool;
    fn tls_ca_certificates(&self) -> Vec<String>;
}

pub async fn create_client<C: ClientConfig, P: CompactJson + Claims>(
    config: &C,
) -> anyhow::Result<openid::Client<Discovered, P>> {
    let mut client = ClientFactory::new();

    if config.tls_insecure() {
        client = client.make_insecure();
    }

    for ca in config.tls_ca_certificates() {
        client = client.add_ca_cert(ca);
    }

    let client = Client::<Discovered, P>::discover_with_client(
        client.build()?,
        config.client_id(),
        config.client_secret(),
        config.redirect_url(),
        config.issuer_url()?,
    )
    .await?;

    log::info!("Discovered OpenID: {:#?}", client.config());

    Ok(client)
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::config::ConfigFromEnv;
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

    #[test]
    fn test_empty_config() {
        AuthenticatorConfig::from_env().expect("Empty config is ok");
    }

    #[test]
    fn test_standard_config() {
        #[derive(Deserialize)]
        struct Config {
            pub oauth: AuthenticatorConfig,
        }

        let mut set = HashMap::new();
        set.insert("OAUTH__SSO_URL", "http://sso.url");

        set.insert("OAUTH__CLIENTS__FOO__CLIENT_ID", "client.id.1");
        set.insert("OAUTH__CLIENTS__FOO__CLIENT_SECRET", "client.secret.1");

        set.insert("OAUTH__CLIENTS__BAR__CLIENT_ID", "client.id.2");
        set.insert("OAUTH__CLIENTS__BAR__CLIENT_SECRET", "");

        let cfg = Config::from_set(set).expect("Config should be ok");

        assert_eq!(cfg.oauth.global.sso_url, Some("http://sso.url".into()));

        assert_eq!(
            cfg.oauth.clients.get("foo"),
            Some(&AuthenticatorClientConfig {
                client_id: "client.id.1".into(),
                client_secret: "client.secret.1".into(),
                scopes: defaults::oauth2_scopes(),
                issuer_url: None,
                tls_insecure: None,
                tls_ca_certificates: None,
            })
        );

        assert_eq!(
            cfg.oauth.clients.get("bar"),
            Some(&AuthenticatorClientConfig {
                client_id: "client.id.2".into(),
                client_secret: "".into(),
                scopes: defaults::oauth2_scopes(),
                issuer_url: None,
                tls_insecure: None,
                tls_ca_certificates: None,
            })
        );
    }
}
