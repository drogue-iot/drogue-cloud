use crate::client::UserAuthClient;
use crate::error::ServiceError;
use crate::openid::Authenticator;
use drogue_client::Context;
use drogue_cloud_service_api::auth::user::authn::{AuthenticationRequest, Outcome};
use drogue_cloud_service_api::auth::user::authz::{AuthorizationRequest, Permission};
use drogue_cloud_service_api::auth::user::{authz, UserInformation};

mod auth_middleware;

// Credentials can either be
//  - username + API key
//  - openID token
//  - Anonymous
pub enum Credentials {
    Token(String),
    ApiKey(UsernameAndApiKey),
    Anonymous,
}

pub struct UsernameAndApiKey {
    pub username: String,
    pub key: Option<String>,
}

#[derive(Clone)]
pub struct Auth {
    pub auth_n: Option<Authenticator>,
    pub auth_z: Option<UserAuthClient>,
    pub permission: Permission,
    pub enable_api_key: bool,
}

impl Auth {
    /// Authorise a request
    pub async fn authenticate_and_authorize(
        &self,
        application: String,
        credentials: Credentials,
    ) -> Result<(), ServiceError> {
        if let (Some(_), Some(_)) = (&self.auth_n, &self.auth_z) {
            let authentication_result = self.authenticate(credentials).await?;

            self.authorize(application, &authentication_result).await

            //authentication disabled
        } else {
            Ok(())
        }
    }

    async fn authenticate(
        &self,
        credentials: Credentials,
    ) -> Result<UserInformation, ServiceError> {
        match credentials {
            Credentials::ApiKey(creds) => {
                if self.enable_api_key {
                    if creds.key.is_none() {
                        log::debug!("Cannot authenticate : empty API key.");
                        return Err(ServiceError::InvalidRequest(String::from(
                            "No API key provided.",
                        )));
                    }

                    let auth_response = self
                        .auth_z
                        .as_ref()
                        .unwrap()
                        .authenticate_api_key(
                            AuthenticationRequest {
                                user_id: creds.username.clone(),
                                api_key: creds.key.clone().unwrap_or_default(),
                            },
                            Context::default(),
                        )
                        .await
                        .map_err(|e| ServiceError::InternalError(e.to_string()))?;
                    match auth_response.outcome {
                        Outcome::Known(details) => Ok(UserInformation::Authenticated(details)),
                        Outcome::Unknown => {
                            log::debug!("Unknown API key");
                            Err(ServiceError::AuthenticationError)
                        }
                    }
                } else {
                    log::debug!("API keys authentication disabled");
                    Err(ServiceError::InvalidRequest(
                        "API keys authentication disabled".to_string(),
                    ))
                }
            }
            Credentials::Token(token) => {
                match &self.auth_n.as_ref().unwrap().validate_token(&token).await {
                    Ok(token) => Ok(UserInformation::Authenticated(token.clone().into())),
                    Err(_) => Err(ServiceError::AuthenticationError),
                }
            }
            Credentials::Anonymous => Ok(UserInformation::Anonymous),
        }
    }

    async fn authorize(
        &self,
        application: String,
        user: &UserInformation,
    ) -> Result<(), ServiceError> {
        log::debug!(
            "Authorizing - user: {:?}, app: {}, permission: {:?}",
            user,
            application,
            self.permission
        );

        let response = self
            .auth_z
            .as_ref()
            .unwrap()
            .authorize(
                AuthorizationRequest {
                    application,
                    permission: self.permission.clone(),
                    user_id: user.user_id().map(ToString::to_string),
                    roles: user.roles().clone(),
                },
                Default::default(),
            )
            .await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?;

        log::debug!("Outcome: {:?}", response);

        match response.outcome {
            authz::Outcome::Allow => Ok(()),
            authz::Outcome::Deny => Err(ServiceError::InvalidRequest(String::from("Unauthorized"))),
        }
    }
}
