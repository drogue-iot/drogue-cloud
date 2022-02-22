pub mod client;
pub mod error;
pub mod mock;

use crate::{defaults, openid::CommaSeparatedVec};
use async_trait::async_trait;
use keycloak::KeycloakAdmin;
use serde::Deserialize;
use url::Url;

#[async_trait]
pub trait KeycloakClient: Clone {
    fn new(config: KeycloakAdminClientConfig) -> Result<Self, error::Error>;

    async fn username_from_id(&self, id: &str) -> Result<String, error::Error>;
    async fn id_from_username(&self, username: &str) -> Result<String, error::Error>;

    async fn admin<'a>(&self) -> Result<KeycloakAdmin, error::Error>;

    fn realm(&self) -> String;
}

#[derive(Clone, Debug, Deserialize)]
pub struct KeycloakAdminClientConfig {
    #[serde(default = "defaults::keycloak_url")]
    pub url: Url,
    #[serde(default = "defaults::realm")]
    pub realm: String,

    pub admin_username: String,
    pub admin_password: String,

    #[serde(default)]
    pub tls_insecure: bool,

    #[serde(default)]
    pub tls_ca_certificates: CommaSeparatedVec,
}
