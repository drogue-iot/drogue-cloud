use crate::client::UserAuthClient;
use crate::error::ServiceError;
use crate::openid::Authenticator;
use chrono::{DateTime, TimeZone, Utc};
use drogue_cloud_service_api::auth::user::{
    authn::{AuthenticationRequest, Outcome},
    UserInformation,
};
use openid::{Claims, CustomClaims};

mod middleware;
pub use middleware::AuthenticatedUntil;

// Credentials can either be
//  - username + Access Token
//  - openID token
//  - Anonymous
pub enum Credentials {
    OpenIDToken(String),
    AccessToken(UsernameAndToken),
    Anonymous,
}

pub struct UsernameAndToken {
    pub username: String,
    pub access_token: Option<String>,
}

/// An Authentication middleware for actix-web relying on drogue-cloud user-auth-service and an openID service
///
/// This middleware will act on each request and try to authenticate the request with :
/// - The `Authorisation: Bearer` header, which should contain an openID token.
/// - The `Authorisation: Basic` header, which should contain a username and an access token issued by the drogue-cloud API.
/// - The `token` query parameter, which should contain am openID token.
///
/// If more than one of the above is provided, the request will be responded with `400: Bad request.`
///
/// After the authentication is successful, this middleware will inject the `UserInformation` in the request object and forward it.
///
/// # Fields
///
/// * `open_id` - An instance of `Authenticator` It's an openID client. It is used to verify OpenID tokens.
/// * `token` - An instance of `UserAuthClient`. It's a client for drogue-cloud-user-auth-service. It is used to verify API keys.
/// * `enable_access_token` - Whether to allow access tokens for authentication.
///
#[derive(Clone)]
pub struct AuthN {
    pub openid: Option<Authenticator>,
    pub token: Option<UserAuthClient>,
    pub enable_access_token: bool,
}

impl AuthN {
    async fn authenticate(
        &self,
        credentials: Credentials,
    ) -> Result<(UserInformation, Option<DateTime<Utc>>), ServiceError> {
        if let (Some(openid), Some(token)) = (&self.openid, &self.token) {
            match credentials {
                Credentials::AccessToken(creds) => {
                    if self.enable_access_token {
                        if creds.access_token.is_none() {
                            log::debug!("Cannot authenticate : empty access token.");
                            return Err(ServiceError::InvalidRequest(String::from(
                                "No access token provided.",
                            )));
                        }
                        let auth_response = token
                            .authenticate_access_token(AuthenticationRequest {
                                user_id: creds.username.clone(),
                                access_token: creds.access_token.clone().unwrap_or_default(),
                            })
                            .await
                            .map_err(|e| ServiceError::InternalError(e.to_string()))?;
                        match auth_response.outcome {
                            Outcome::Known(details) => {
                                Ok((UserInformation::Authenticated(details), None))
                            }
                            Outcome::Unknown => {
                                log::debug!("Unknown access token");
                                Err(ServiceError::AuthenticationError)
                            }
                        }
                    } else {
                        log::debug!("Access token authentication disabled");
                        Err(ServiceError::InvalidRequest(
                            "Access token authentication disabled".to_string(),
                        ))
                    }
                }
                Credentials::OpenIDToken(token) => match openid.validate_token(&token).await {
                    Ok(token) => Ok((
                        UserInformation::Authenticated(token.clone().into()),
                        Some(Utc.timestamp(token.standard_claims().exp(), 0)),
                    )),
                    Err(err) => {
                        log::debug!("Authentication error: {err}");
                        Err(ServiceError::AuthenticationError)
                    }
                },
                Credentials::Anonymous => Ok((UserInformation::Anonymous, None)),
            }
        } else {
            //authentication disabled
            Ok((UserInformation::Anonymous, None))
        }
    }
}
