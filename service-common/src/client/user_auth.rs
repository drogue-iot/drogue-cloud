use crate::{auth::openid::TokenConfig, defaults, reqwest::ClientFactory};
use drogue_client::user;
use serde::Deserialize;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct UserAuthClientConfig {
    #[serde(default = "defaults::user_auth_url")]
    pub url: Url,

    #[serde(flatten, default)]
    pub token_config: Option<TokenConfig>,
}

impl UserAuthClientConfig {
    pub async fn into_client(self) -> anyhow::Result<user::v1::Client> {
        let token = if let Some(token) = self.token_config {
            Some(token.discover_from().await?)
        } else {
            None
        };

        let authn_url = self.url.join("/api/user/v1alpha1/authn")?;
        let authz_url = self.url.join("/api/v1/user/authz")?;

        Ok(user::v1::Client::new(
            ClientFactory::new().build()?,
            authn_url,
            authz_url,
            token,
        ))
    }
}
