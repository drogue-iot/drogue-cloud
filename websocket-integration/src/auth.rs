use drogue_client::Context;
use drogue_cloud_service_api::auth::user::authn::{AuthenticationRequest, Outcome};
use drogue_cloud_service_api::auth::user::authz::{AuthorizationRequest, Permission};
use drogue_cloud_service_api::auth::user::{authz, UserInformation};
use drogue_cloud_service_common::client::UserAuthClient;
use drogue_cloud_service_common::error::ServiceError;
use drogue_cloud_service_common::openid::Authenticator;
use std::sync::Arc;

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

impl Credentials {
    pub async fn authenticate_and_authorize(
        &self,
        application: String,
        authz: &Arc<UserAuthClient>,
        auth: Authenticator,
    ) -> Result<UserInformation, ServiceError> {
        let authentication_result = self.authenticate(auth, authz).await?;

        Credentials::authorize(application, &authentication_result, Permission::Read, authz)
            .await
            .map(|_| authentication_result)
    }

    async fn authenticate(
        &self,
        auth: Authenticator,
        authz: &Arc<UserAuthClient>,
    ) -> Result<UserInformation, ServiceError> {
        match self {
            Credentials::ApiKey(creds) => {
                if creds.key.is_none() {
                    log::debug!("Cannot authenticate : empty API key.");
                    return Err(ServiceError::InvalidRequest(String::from(
                        "No API key provided.",
                    )));
                }

                let auth_response = authz
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
            }
            Credentials::Token(token) => match auth.validate_token(&token).await {
                Ok(token) => Ok(UserInformation::Authenticated(token.into())),
                Err(_) => Err(ServiceError::AuthenticationError),
            },
            Credentials::Anonymous => Ok(UserInformation::Anonymous),
        }
    }

    async fn authorize(
        application: String,
        user: &UserInformation,
        permission: Permission,
        authz_client: &Arc<UserAuthClient>,
    ) -> Result<(), ServiceError> {
        log::debug!(
            "Authorizing - user: {:?}, app: {}, permission: {:?}",
            user,
            application,
            permission
        );

        let response = authz_client
            .authorize(
                AuthorizationRequest {
                    application,
                    permission,
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
