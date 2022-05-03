use crate::{defaults, openid::TokenConfig, reqwest::ClientFactory};
use drogue_client::registry;
use serde::Deserialize;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct RegistryConfig {
    #[serde(default = "defaults::registry_url")]
    pub url: Url,
    #[serde(flatten)]
    pub token_config: Option<TokenConfig>,
}

impl RegistryConfig {
    pub async fn into_client(self) -> anyhow::Result<registry::v1::Client> {
        let token = if let Some(token) = self.token_config {
            Some(token.discover_from().await?)
        } else {
            None
        };

        Ok(registry::v1::Client::new(
            ClientFactory::new().build()?,
            self.url,
            token,
        ))
    }
}
