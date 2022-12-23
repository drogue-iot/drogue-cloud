use async_trait::async_trait;
use chrono::Utc;
use drogue_cloud_service_api::webapp::ResponseError;
use drogue_cloud_service_api::{
    admin::Roles,
    auth::user::{UserDetails, UserInformation},
    token::{AccessToken, AccessTokenCreationOptions, AccessTokenData, CreatedAccessToken},
};
use drogue_cloud_service_common::keycloak::{error::Error, KeycloakClient};
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::HashMap;

const ATTR_PREFIX: &str = "access_token_";

#[async_trait]
pub trait AccessTokenService: Clone {
    type Error: ResponseError;

    async fn create(
        &self,
        identity: &UserInformation,
        opts: AccessTokenCreationOptions,
        registry: &drogue_client::registry::v1::Client,
    ) -> Result<CreatedAccessToken, Self::Error>;
    async fn delete(&self, identity: &UserInformation, prefix: String) -> Result<(), Self::Error>;
    async fn list(&self, identity: &UserInformation) -> Result<Vec<AccessToken>, Self::Error>;

    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<UserDetails>, Self::Error>;
}

#[derive(Clone)]
pub struct KeycloakAccessTokenService<K: KeycloakClient> {
    pub client: K,
}

impl<K: KeycloakClient> KeycloakAccessTokenService<K> {
    fn insert_entry(
        attributes: &mut HashMap<String, Value>,
        prefix: String,
        entry: AccessTokenData,
    ) -> Result<(), Error> {
        let key = Self::make_key(prefix);
        // although the map claims to allow any value, it actually only accepts strings.
        attributes.insert(key, Value::String(serde_json::to_string(&entry)?));
        Ok(())
    }

    fn make_key(prefix: String) -> String {
        format!("{}{}", ATTR_PREFIX, prefix)
    }

    /// Decode a keycloak attribute value into an [`AccessTokenData`], if possible.
    ///
    /// If the attribute value is of the wrong type, empty, or fails to decide, an error is returned.
    fn decode_data(value: Value) -> Result<AccessTokenData, Error> {
        value
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .map_or_else(
                || Err(Error::NotAuthorized),
                |str| Ok(serde_json::from_str::<AccessTokenData>(str)?),
            )
    }
}

#[async_trait]
impl<K> AccessTokenService for KeycloakAccessTokenService<K>
where
    K: KeycloakClient + std::marker::Sync + std::marker::Send,
{
    type Error = Error;

    async fn create(
        &self,
        identity: &UserInformation,
        opts: AccessTokenCreationOptions,
        registry: &drogue_client::registry::v1::Client,
    ) -> Result<CreatedAccessToken, Self::Error> {
        let user_id = match identity.user_id() {
            Some(user_id) => user_id,
            None => return Err(Error::NotAuthorized),
        };

        let token = crate::rng::generate_access_token();
        let admin = self.client.admin().await?;

        let mut user = admin
            .realm_users_with_id_get(&self.client.realm(), user_id)
            .await?;

        // if there are no active claims attached to the current identity
        // there is no need to limit these new claims
        let claims = if let Some(roles) = identity.token_claims() {
            if let Some(mut claims) = opts.claims {
                // 1 - Prevent tokens claims escalation
                // i.e. creating a token with more permissions than the current token

                // if the current token is not allowed to create apps
                // that new one should not either
                if roles.create != claims.create {
                    return Err(Error::NotAuthorized);
                }
                // if the current token is not allowed to do some tokens operations
                // that new one should not either
                for token_permission in claims.tokens.iter() {
                    if !roles.tokens.contains(token_permission) {
                        return Err(Error::NotAuthorized);
                    }
                }

                let mut mapped_claims: IndexMap<String, Roles> = IndexMap::new();
                // 2 - prevent claims escalation for applications
                for (app_name, claims) in claims.applications.iter() {
                    // 2a - retrieve the app object
                    let app = registry
                        .get_app(app_name)
                        .await
                        .map_err(|e| {
                            Error::Internal(format!("Error with the registry client {}", e))
                        })?
                        .ok_or(Error::NotFound)?;

                    // 2b - get the app uuid from the app name
                    let app_id = app.metadata.uid.clone();

                    // 2c - if the used tokens does not have claims for this app
                    // we cannot create a new token that does
                    match roles.applications.get(&app_id) {
                        Some(roles) => {
                            for claimed_role in claims.0.iter() {
                                if !roles.contains(claimed_role) {
                                    return Err(Error::NotAuthorized);
                                }
                            }
                        }
                        None => return Err(Error::NotAuthorized),
                    }

                    mapped_claims.insert(app_id, claims.clone());
                }
                claims.applications = mapped_claims;
                Some(claims)
            } else {
                None
            }
        } else {
            opts.claims
        };

        let insert = AccessTokenData {
            hashed_token: token.1,
            created: Utc::now(),
            description: opts.description,
            claims,
        };

        let prefix = &token.0.prefix;

        if let Some(ref mut attributes) = user.attributes {
            Self::insert_entry(attributes, prefix.clone(), insert)?;
        } else {
            let mut attributes = HashMap::new();
            Self::insert_entry(&mut attributes, prefix.clone(), insert)?;
            user.attributes = Some(attributes);
        }

        admin
            .realm_users_with_id_put(&self.client.realm(), user_id, user)
            .await?;

        Ok(token.0)
    }

    async fn delete(&self, identity: &UserInformation, prefix: String) -> Result<(), Self::Error> {
        let user_id = match identity.user_id() {
            Some(user_id) => user_id,
            None => return Err(Error::NotAuthorized),
        };

        let admin = &self.client.admin().await?;

        let mut user = admin
            .realm_users_with_id_get(&self.client.realm(), user_id)
            .await?;

        let changed = if let Some(ref mut attributes) = user.attributes {
            let key = Self::make_key(prefix);
            attributes.remove(&key).is_some()
        } else {
            false
        };

        if changed {
            admin
                .realm_users_with_id_put(&self.client.realm(), user_id, user)
                .await?;
        }

        Ok(())
    }

    async fn list(&self, identity: &UserInformation) -> Result<Vec<AccessToken>, Self::Error> {
        let user_id = match identity.user_id() {
            Some(user_id) => user_id,
            None => return Err(Error::NotAuthorized),
        };

        let admin = self.client.admin().await?;

        let user = admin
            .realm_users_with_id_get(&self.client.realm(), user_id)
            .await?;

        let tokens = if let Some(attributes) = user.attributes {
            let mut tokens = Vec::new();
            for (key, value) in attributes {
                log::debug!("{}, {:?}", key, value);
                if let Some(prefix) = key.strip_prefix(ATTR_PREFIX) {
                    log::debug!("Matches - prefix: {}", prefix);
                    match Self::decode_data(value) {
                        Ok(data) => {
                            tokens.push(AccessToken {
                                prefix: prefix.into(),
                                created: data.created,
                                description: data.description,
                                claims: data.claims,
                            });
                        }
                        or => log::debug!("Value: {:?}", or),
                    }
                }
            }
            tokens
        } else {
            vec![]
        };

        Ok(tokens)
    }

    async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<UserDetails>, Self::Error> {
        // check if the token appears valid (format, checksum, ...)

        let prefix = if let Some(prefix) = crate::rng::is_valid(password) {
            prefix
        } else {
            return Ok(None);
        };

        // load the user

        let admin = self.client.admin().await?;
        let user = admin
            .realm_users_get(
                &self.client.realm(),
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
            .pop();

        log::debug!("Found user: {:?}", user);

        let user = match user {
            Some(user) => user,
            None => return Ok(None),
        };

        let user_id = match user.id {
            Some(user_id) => user_id,
            None => return Ok(None),
        };

        // extract the entry

        let key = Self::make_key(prefix.to_owned());

        log::debug!("Looking for attribute: {}", key);

        let (expected_hash, claims) = match user.attributes.and_then(|mut a| a.remove(&key)) {
            Some(value) => match Self::decode_data(value) {
                Ok(data) => (data.hashed_token, data.claims),
                Err(_) => return Ok(None),
            },
            None => return Ok(None),
        };

        // verify the hash

        log::debug!("Password: {}", password);
        let provided_hash = crate::rng::hash_token(password);
        log::debug!(
            "Comparing hashes - expected: {}, provided: {}",
            expected_hash,
            provided_hash
        );

        Ok(match provided_hash == expected_hash {
            true => {
                let details = UserDetails {
                    user_id,
                    roles: vec![], // FIXME: we should be able to store scopes/roles as well,
                    claims,
                };
                Some(details)
            }
            false => None,
        })
    }
}
