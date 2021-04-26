use crate::data::ApiKeyCreationOptions;
use crate::{
    data::{ApiKey, ApiKeyCreated, ApiKeyData},
    error::Error,
};
use actix_web::ResponseError;
use async_trait::async_trait;
use chrono::Utc;
use drogue_cloud_service_common::{auth::Identity, defaults};
use keycloak::{KeycloakAdmin, KeycloakAdminToken};
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use url::Url;

const ATTR_PREFIX: &str = "api_key";

#[async_trait]
pub trait ApiKeyService: Clone {
    type Error: ResponseError;

    async fn create(
        &self,
        identity: &dyn Identity,
        opts: ApiKeyCreationOptions,
    ) -> Result<ApiKeyCreated, Self::Error>;
    async fn delete(&self, identity: &dyn Identity, prefix: String) -> Result<(), Self::Error>;
    async fn list(&self, identity: &dyn Identity) -> Result<Vec<ApiKey>, Self::Error>;

    async fn authenticate(&self, username: &str, password: &str) -> Result<bool, Self::Error>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct KeycloakApiKeyServiceConfig {
    #[serde(default = "defaults::keycloak_url")]
    pub keycloak_url: Url,
    #[serde(default = "defaults::realm")]
    pub realm: String,

    pub admin_username: String,
    pub admin_password: String,
}

#[derive(Clone)]
pub struct KeycloakApiKeyService {
    client: reqwest::Client,
    url: String,
    realm: String,
    admin_username: String,
    admin_password: String,
}

impl KeycloakApiKeyService {
    pub fn new(config: KeycloakApiKeyServiceConfig) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            url: config.keycloak_url.into_string(),
            realm: config.realm,
            admin_username: config.admin_username,
            admin_password: config.admin_password,
        })
    }

    async fn token<'a>(&self) -> Result<KeycloakAdminToken<'a>, Error> {
        // FIXME: should cache and refresh token
        Ok(KeycloakAdminToken::acquire(
            &self.url,
            &self.admin_username,
            &self.admin_password,
            &self.client,
        )
        .await?)
    }

    async fn admin<'a>(&self) -> Result<KeycloakAdmin<'a>, Error> {
        let token = self.token().await?;
        Ok(KeycloakAdmin::new(&self.url, token, self.client.clone()))
    }

    fn insert_entry(
        attributes: &mut HashMap<Cow<str>, Value>,
        prefix: String,
        entry: ApiKeyData,
    ) -> Result<(), Error> {
        let key = Self::make_key(prefix);
        attributes.insert(key, serde_json::to_value(&entry)?);
        Ok(())
    }

    fn make_key(prefix: String) -> Cow<'static, str> {
        Cow::Owned(format!("{}_{}", ATTR_PREFIX, prefix))
    }
}

#[async_trait]
impl ApiKeyService for KeycloakApiKeyService {
    type Error = Error;

    async fn create(
        &self,
        identity: &dyn Identity,
        opts: ApiKeyCreationOptions,
    ) -> Result<ApiKeyCreated, Self::Error> {
        let user_id = match identity.user_id() {
            Some(user_id) => user_id,
            None => return Err(Error::NotAuthorized),
        };

        let key = crate::rng::generate_api_key();
        let admin = self.admin().await?;

        let mut user = admin.realm_users_with_id_get(&self.realm, user_id).await?;

        let insert = ApiKeyData {
            hashed_key: key.1,
            created: Utc::now(),
            description: opts.description,
        };

        let prefix = &key.0.prefix;

        if let Some(ref mut attributes) = user.attributes {
            Self::insert_entry(attributes, prefix.clone(), insert)?;
        } else {
            let mut attributes = HashMap::new();
            Self::insert_entry(&mut attributes, prefix.clone(), insert)?;
            user.attributes = Some(attributes);
        }

        admin.realm_users_post(&self.realm, user).await?;

        Ok(key.0)
    }

    async fn delete(&self, identity: &dyn Identity, prefix: String) -> Result<(), Self::Error> {
        let user_id = match identity.user_id() {
            Some(user_id) => user_id,
            None => return Err(Error::NotAuthorized),
        };

        let admin = self.admin().await?;

        let mut user = admin.realm_users_with_id_get(&self.realm, user_id).await?;

        let changed = if let Some(ref mut attributes) = user.attributes {
            let key = Cow::Owned(format!("{}_{}", ATTR_PREFIX, prefix));
            attributes.remove(&key).is_some()
        } else {
            false
        };

        if changed {
            admin.realm_users_post(&self.realm, user).await?;
        }

        Ok(())
    }

    async fn list(&self, identity: &dyn Identity) -> Result<Vec<ApiKey>, Self::Error> {
        let user_id = match identity.user_id() {
            Some(user_id) => user_id,
            None => return Err(Error::NotAuthorized),
        };

        let admin = self.admin().await?;

        let user = admin.realm_users_with_id_get(&self.realm, user_id).await?;

        let keys = if let Some(attributes) = user.attributes {
            let mut keys = Vec::new();
            for (key, value) in attributes {
                if let Some(prefix) = key.strip_prefix("drg_api_key_") {
                    if let Ok(data) = serde_json::from_value::<ApiKeyData>(value) {
                        keys.push(ApiKey {
                            prefix: prefix.into(),
                            created: data.created,
                            description: data.description,
                        })
                    }
                }
            }
            keys
        } else {
            vec![]
        };

        Ok(keys)
    }

    async fn authenticate(&self, username: &str, password: &str) -> Result<bool, Self::Error> {
        // check if the key appears valid (format, checksum, ...)

        let prefix = if let Some(prefix) = crate::rng::is_valid(&password) {
            prefix
        } else {
            return Ok(false);
        };

        // load the user

        let admin = self.admin().await?;
        let user = admin
            .realm_users_with_id_get(&self.realm, &username)
            .await?;

        // extract the entry

        let key = Self::make_key(prefix.to_owned());
        let expected_hash = match user.attributes.and_then(|mut a| a.remove(&key)) {
            Some(attributes) => match serde_json::from_value::<ApiKeyData>(attributes) {
                Ok(data) => data.hashed_key,
                Err(_) => return Ok(false),
            },
            None => return Ok(false),
        };

        // verify the hash

        let actual_hash = crate::rng::hash_key(&password);
        Ok(actual_hash == expected_hash)
    }
}
