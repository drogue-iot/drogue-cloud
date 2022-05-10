use crate::{
    keycloak::{error::Error, KeycloakAdminClientConfig, KeycloakClient},
    reqwest::ClientFactory,
};
use async_trait::async_trait;
use keycloak::{KeycloakAdmin, KeycloakAdminToken};

#[derive(Clone)]
pub struct KeycloakAdminClient {
    client: reqwest::Client,
    url: String,
    pub realm: String,
    admin_username: String,
    admin_password: String,
}

#[async_trait]
impl KeycloakClient for KeycloakAdminClient {
    fn new(config: KeycloakAdminClientConfig) -> Result<Self, Error> {
        let mut client = ClientFactory::new();

        if config.tls_insecure {
            client = client.make_insecure();
        }

        client = client.add_ca_certs(&config.tls_ca_certificates.0);

        Ok(Self {
            client: client
                .build()
                .map_err(|err| Error::Internal(format!("Failed to create client: {err}")))?,
            url: {
                let url: String = config.url.into();
                url.trim_end_matches('/').into()
            },
            realm: config.realm,
            admin_username: config.admin_username,
            admin_password: config.admin_password,
        })
    }

    async fn username_from_id(&self, id: &str) -> Result<String, Error> {
        match self
            .admin()
            .await?
            .realm_users_with_id_get(&self.realm, id)
            .await
        {
            // fixme Is the unwrap unsafe ? The user should always have a username
            Ok(user) => Ok(user.username.unwrap()),
            Err(_) => Err(Error::NotFound),
        }
    }

    async fn id_from_username(&self, username: &str) -> Result<String, Error> {
        match self
            .admin()
            .await?
            .realm_users_get(
                &self.realm,
                None,
                None,
                None,
                Some(true),
                Some(true),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(username.to_string()),
            )
            .await?
            .pop()
        {
            Some(user) => Ok(user.id.unwrap()),
            None => Err(Error::NotFound),
        }
    }

    async fn admin<'a>(&self) -> Result<KeycloakAdmin, Error> {
        let token = self.token().await?;
        Ok(KeycloakAdmin::new(&self.url, token, self.client.clone()))
    }

    fn realm(&self) -> String {
        self.realm.clone()
    }
}

impl KeycloakAdminClient {
    async fn token<'a>(&self) -> Result<KeycloakAdminToken, Error> {
        // Refresh token if needed is WIP.
        Ok(KeycloakAdminToken::acquire(
            &self.url,
            &self.admin_username,
            &self.admin_password,
            &self.client,
        )
        .await?)
    }
}
