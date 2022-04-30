use crate::defaults;
use crate::reqwest::ClientFactory;
use core::fmt::Debug;
use drogue_client::openid::OpenIdTokenProvider;
use serde::Deserialize;
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use url::Url;

/// All required configuration when authentication is enabled.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TokenConfig {
    pub client_id: String,

    pub client_secret: String,

    #[serde(default)]
    pub issuer_url: Option<Url>,

    #[serde(default)]
    pub sso_url: Option<Url>,

    #[serde(default)]
    pub tls_insecure: bool,

    #[serde(default)]
    pub tls_ca_certificates: CommaSeparatedVec,

    #[serde(default = "defaults::realm")]
    pub realm: String,

    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub refresh_before: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Deserialize)]
#[serde(from = "String")]
pub struct CommaSeparatedVec(pub Vec<String>);

impl Deref for CommaSeparatedVec {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CommaSeparatedVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<String>> for CommaSeparatedVec {
    fn from(values: Vec<String>) -> Self {
        Self(values)
    }
}

impl From<String> for CommaSeparatedVec {
    fn from(value: String) -> Self {
        Self(value.split(",").map(|s| s.into()).collect::<Vec<String>>())
    }
}

impl TokenConfig {
    pub fn issuer_url(&self) -> anyhow::Result<Url> {
        match (&self.issuer_url, &self.sso_url) {
            (Some(issuer_url), _) => Ok(issuer_url.clone()),
            (None, Some(sso_url)) => {
                // keycloak
                Ok(sso_url.join(&format!("realms/{realm}", realm = self.realm))?)
            }
            _ => {
                anyhow::bail!(
                    "Invalid token provider configuration, need either issuer or SSO url"
                );
            }
        }
    }
}

impl TokenConfig {
    pub async fn into_client(self, redirect: Option<String>) -> anyhow::Result<openid::Client> {
        let issuer = self.issuer_url()?;

        let mut client = ClientFactory::new();
        client = client.add_ca_certs(self.tls_ca_certificates.0);

        if self.tls_insecure {
            client = client.make_insecure();
        }

        Ok(openid::Client::discover_with_client(
            client.build()?,
            self.client_id,
            self.client_secret,
            redirect,
            issuer,
        )
        .await?)
    }

    /// Create a new provider by discovering the OAuth2 client from the configuration
    pub async fn discover_from(self) -> anyhow::Result<OpenIdTokenProvider> {
        let refresh_before = self
            .refresh_before
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .unwrap_or_else(|| chrono::Duration::seconds(15));

        Ok(OpenIdTokenProvider::new(
            self.into_client(None).await?,
            refresh_before,
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::ConfigFromEnv;
    use std::collections::HashMap;

    #[test]
    fn test_missing_urls() {
        let config = TokenConfig {
            client_id: "".to_string(),
            client_secret: "".to_string(),
            issuer_url: None,
            sso_url: None,
            realm: "".into(),
            refresh_before: None,
            tls_insecure: false,
            tls_ca_certificates: vec![].into(),
        };

        let url = config.issuer_url();
        assert!(url.is_err());
    }

    #[test]
    fn test_issuer_url() {
        let config = TokenConfig {
            client_id: "".to_string(),
            client_secret: "".to_string(),
            issuer_url: None,
            sso_url: Some(Url::parse("http://foo.bar/baz/buz").unwrap()),
            realm: "drogue".to_string(),
            refresh_before: None,
            tls_insecure: false,
            tls_ca_certificates: vec![].into(),
        };

        let url = config.issuer_url();
        assert!(url.is_ok());
        assert_eq!(
            // sso URL doesn't end with a slash, which makes it drop the last path element
            "http://foo.bar/baz/realms/drogue",
            url.unwrap().to_string()
        );
    }

    #[test]
    fn test_ca_certs() {
        let mut envs = HashMap::new();

        envs.insert("CLIENT_ID", "id");
        envs.insert("CLIENT_SECRET", "secret");
        envs.insert("SSO_URL", "http://foo.bar/baz/buz");
        envs.insert("REALM", "drogue");
        envs.insert("TLS_CA_CERTIFICATES", "/foo/bar/baz");

        let config = TokenConfig::from_set(envs).unwrap();

        assert_eq!(
            TokenConfig {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
                issuer_url: None,
                sso_url: Some(Url::parse("http://foo.bar/baz/buz").unwrap()),
                realm: "drogue".to_string(),
                refresh_before: None,
                tls_insecure: false,
                tls_ca_certificates: vec!["/foo/bar/baz".to_string()].into(),
            },
            config
        );
    }

    #[test]
    fn test_ca_certs_multi() {
        let mut envs = HashMap::new();

        envs.insert("CLIENT_ID", "id");
        envs.insert("CLIENT_SECRET", "secret");
        envs.insert("SSO_URL", "http://foo.bar/baz/buz");
        envs.insert("REALM", "drogue");
        envs.insert("TLS_CA_CERTIFICATES", "/foo/bar/baz,/foo/bar/baz2");

        let config = TokenConfig::from_set(envs).unwrap();

        assert_eq!(
            TokenConfig {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
                issuer_url: None,
                sso_url: Some(Url::parse("http://foo.bar/baz/buz").unwrap()),
                realm: "drogue".to_string(),
                refresh_before: None,
                tls_insecure: false,
                tls_ca_certificates: vec!["/foo/bar/baz".to_string(), "/foo/bar/baz2".to_string()]
                    .into(),
            },
            config
        );
    }
}
