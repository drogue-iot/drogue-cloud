use crate::client::UserAuthClient;
use crate::error::ServiceError;
use crate::openid::Authenticator;
use drogue_client::Context;
use drogue_cloud_service_api::auth::user::authn::{AuthenticationRequest, Outcome};
use drogue_cloud_service_api::auth::user::UserInformation;

mod middleware;

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
pub struct AuthN {
    pub openid: Option<Authenticator>,
    pub token: Option<UserAuthClient>,
    pub enable_api_key: bool,
}

impl AuthN {
    async fn authenticate(
        &self,
        credentials: Credentials,
    ) -> Result<UserInformation, ServiceError> {
        if let (Some(openid), Some(token)) = (&self.openid, &self.token) {
            match credentials {
                Credentials::ApiKey(creds) => {
                    if self.enable_api_key {
                        if creds.key.is_none() {
                            log::debug!("Cannot authenticate : empty API key.");
                            return Err(ServiceError::InvalidRequest(String::from(
                                "No API key provided.",
                            )));
                        }

                        let auth_response = token
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
                Credentials::Token(token) => match openid.validate_token(&token).await {
                    Ok(token) => Ok(UserInformation::Authenticated(token.clone().into())),
                    Err(_) => Err(ServiceError::AuthenticationError),
                },
                Credentials::Anonymous => Ok(UserInformation::Anonymous),
            }
            //authentication disabled
        } else {
            Ok(UserInformation::Anonymous)
        }
    }
}
