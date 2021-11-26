use crate::{defaults, openid::TokenConfig};
use drogue_client::{openid::OpenIdTokenProvider, registry};
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
    pub async fn into_client(
        self,
        client: reqwest::Client,
    ) -> anyhow::Result<registry::v1::Client<Option<OpenIdTokenProvider>>> {
        let token = if let Some(token) = self.token_config {
            Some(token.discover_from(client.clone()).await?)
        } else {
            None
        };
        Ok(registry::v1::Client::new(client, self.url, token))
    }
}
