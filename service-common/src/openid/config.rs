use crate::defaults;
use core::fmt::Debug;
use drogue_client::openid::OpenIdTokenProvider;
use serde::Deserialize;
use std::time::Duration;
use url::Url;

/// All required configuration when authentication is enabled.
#[derive(Clone, Debug, Deserialize)]
pub struct TokenConfig {
    pub client_id: String,

    pub client_secret: String,

    #[serde(default)]
    pub issuer_url: Option<Url>,

    #[serde(default)]
    pub sso_url: Option<Url>,

    #[serde(default = "defaults::realm")]
    pub realm: String,

    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub refresh_before: Option<Duration>,
}

impl TokenConfig {
    /// pull in global configuration options
    pub fn amend_with_env(mut self) -> Self {
        // try fetching global SSO url
        if self.sso_url.is_none() {
            self.sso_url = std::env::var("SSO_URL")
                .ok()
                .and_then(|url| Url::parse(&url).ok());
        }

        self
    }

    pub fn issuer_url(&self) -> anyhow::Result<Url> {
        match (&self.issuer_url, &self.sso_url) {
            (Some(issuer_url), _) => Ok(issuer_url.clone()),
            (None, Some(sso_url)) => {
                // keycloak
                Ok(sso_url.join(&format!("auth/realms/{realm}", realm = self.realm))?)
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
    pub async fn into_client(
        self,
        client: reqwest::Client,
        redirect: Option<String>,
    ) -> anyhow::Result<openid::Client> {
        let issuer = self.issuer_url()?;

        Ok(openid::Client::discover_with_client(
            client,
            self.client_id,
            self.client_secret,
            redirect,
            issuer,
        )
        .await?)
    }

    /// Create a new provider by discovering the OAuth2 client from the configuration
    pub async fn discover_from(
        self,
        client: reqwest::Client,
    ) -> anyhow::Result<OpenIdTokenProvider> {
        let refresh_before = self
            .refresh_before
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .unwrap_or_else(|| chrono::Duration::seconds(15));

        Ok(OpenIdTokenProvider::new(
            self.into_client(client, None).await?,
            refresh_before,
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_missing_urls() {
        let config = TokenConfig {
            client_id: "".to_string(),
            client_secret: "".to_string(),
            issuer_url: None,
            sso_url: None,
            realm: "".into(),
            refresh_before: None,
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
        };

        let url = config.issuer_url();
        assert!(url.is_ok());
        assert_eq!(
            // sso URL doesn't end with a slash, which makes it drop the last path element
            "http://foo.bar/baz/auth/realms/drogue",
            url.unwrap().to_string()
        );
    }
}
