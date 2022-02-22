use crate::keycloak::{error::Error, KeycloakAdminClientConfig, KeycloakClient};

use async_trait::async_trait;
use keycloak::KeycloakAdmin;
use url::Url;

#[derive(Clone)]
pub struct KeycloakAdminMock;

#[async_trait]
impl KeycloakClient for KeycloakAdminMock {
    fn new(_: KeycloakAdminClientConfig) -> Result<Self, Error> {
        Ok(KeycloakAdminMock)
    }

    async fn username_from_id(&self, id: &str) -> Result<String, Error> {
        Ok(id.to_string())
    }

    async fn id_from_username(&self, username: &str) -> Result<String, Error> {
        Ok(username.to_string())
    }

    async fn admin<'a>(&self) -> Result<KeycloakAdmin, Error> {
        todo!()
    }

    fn realm(&self) -> String {
        String::from("mock-realm")
    }
}

impl KeycloakAdminClientConfig {
    pub fn mock() -> Self {
        KeycloakAdminClientConfig {
            url: Url::parse("https://drogue.io/").unwrap(),
            realm: "mock".to_string(),
            admin_username: "admin".to_string(),
            admin_password: "password".to_string(),
            tls_insecure: false,
            tls_ca_certificates: vec![].into(),
        }
    }
}
