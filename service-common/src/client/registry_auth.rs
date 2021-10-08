use crate::{config::ConfigFromEnv, defaults, openid::TokenConfig};
use drogue_client::registry;
use serde::Deserialize;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct RegistryConfig {
    #[serde(default = "defaults::registry_url")]
    pub url: Url,
    pub token_config: Option<TokenConfig>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: defaults::registry_url(),
            token_config: TokenConfig::from_env_prefix("REGISTRY")
                .map(|v| v.amend_with_env())
                .ok(),
        }
    }
}

impl RegistryConfig {
    pub async fn into_client(
        self,
        client: reqwest::Client,
    ) -> anyhow::Result<registry::v1::Client> {
        let token = if let Some(token) = self.token_config {
            Some(token.discover_from(client.clone()).await?)
        } else {
            None
        };
        Ok(registry::v1::Client::new(client, self.url, token))
    }
}
