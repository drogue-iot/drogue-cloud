use crate::openid::Expires;
use async_std::sync::RwLock;
use core::fmt::{self, Debug, Formatter};
use serde::Deserialize;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// A provider which provides access tokens for services.
#[derive(Clone)]
pub struct OpenIdTokenProvider {
    client: Arc<openid::Client>,
    current_token: Arc<RwLock<Option<openid::Bearer>>>,
    refresh_before: chrono::Duration,
}

impl Debug for OpenIdTokenProvider {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenProvider")
            .field(
                "client",
                &format!("{} / {:?}", self.client.client_id, self.client.http_client),
            )
            .field("current_token", &"...")
            .finish()
    }
}

/// All required configuration when authentication is enabled.
#[derive(Clone, Debug, Deserialize)]
pub struct TokenConfig {
    pub client_id: String,

    pub client_secret: String,

    #[serde(default)]
    pub issuer_url: Option<Url>,

    #[serde(default)]
    pub sso_url: Option<Url>,

    #[serde(default = "default_realm")]
    pub realm: String,

    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub refresh_before: Option<Duration>,
}

fn default_realm() -> String {
    "drogue".into()
}

impl TokenConfig {
    pub fn issuer_url(&self) -> anyhow::Result<Url> {
        match (&self.issuer_url, &self.sso_url) {
            (Some(issuer_url), _) => Ok(issuer_url.clone()),
            (None, Some(sso_url)) => {
                // keycloak
                Ok(sso_url.join(&format!("auth/realms/{realm}", realm = self.realm))?)
            }
            _ => {
                anyhow::bail!(
                    "Invalid token provider configuration, need either 'ISSUER_URL' or  'SSO_URL'"
                );
            }
        }
    }
}

impl OpenIdTokenProvider {
    /// Create a new provider using the provided client.
    pub fn new(client: openid::Client, refresh_before: chrono::Duration) -> Self {
        Self {
            client: Arc::new(client),
            current_token: Arc::new(RwLock::new(None)),
            refresh_before,
        }
    }

    /// Create a new provider by discovering the OAuth2 client.
    pub async fn discover(
        id: String,
        secret: String,
        issuer: Url,
        refresh_before: chrono::Duration,
    ) -> Result<Self, openid::error::Error> {
        let client = openid::Client::discover(id, secret, None, issuer).await?;

        Ok(Self::new(client, refresh_before))
    }

    pub async fn discover_from(config: TokenConfig) -> anyhow::Result<Self> {
        let issuer_url = config.issuer_url()?;
        let refresh_before = config
            .refresh_before
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .unwrap_or_else(|| chrono::Duration::seconds(15));
        Ok(Self::discover(
            config.client_id,
            config.client_secret,
            issuer_url,
            refresh_before,
        )
        .await?)
    }

    /// return a fresh token, this may be an existing (non-expired) token
    /// a newly refreshed token.
    pub async fn provide_token(&self) -> Result<openid::Bearer, openid::error::Error> {
        match self.current_token.read().await.deref() {
            Some(token) if !token.expires_before(self.refresh_before) => return Ok(token.clone()),
            _ => {}
        }

        // fetch fresh token after releasing the read lock

        self.fetch_fresh_token().await
    }

    async fn fetch_fresh_token(&self) -> Result<openid::Bearer, openid::error::Error> {
        let mut lock = self.current_token.write().await;

        match lock.deref() {
            // check if someone else refreshed the token in the meantime
            Some(token) if !token.expires_before(self.refresh_before) => return Ok(token.clone()),
            _ => {}
        }

        // we hold the write-lock now, and can perform the refresh operation

        let next_token = match lock.take() {
            // if we don't have any token, fetch an initial one
            None => {
                log::debug!("Fetching initial token... ");
                self.initial_token().await?
            }
            // if we have an expired one, refresh it
            Some(current_token) => {
                log::debug!("Refreshing token ... ");
                match current_token.refresh_token.is_some() {
                    true => self.client.refresh_token(current_token, None).await?,
                    false => self.initial_token().await?,
                }
            }
        };

        log::debug!("Next token: {:?}", next_token);

        lock.replace(next_token.clone());

        // done

        Ok(next_token)
    }

    async fn initial_token(&self) -> Result<openid::Bearer, openid::error::Error> {
        Ok(self.client.request_token_using_client_credentials().await?)
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
