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

/// An Authentication middleware for actix-web relying on drogue-cloud user-auth-service and an openID service
///
/// This middleware will act on each request and try to authenticate the request with :
/// - The `Authorisation: Bearer` header, which should contain an openID token.
/// - The `Authorisation: Basic` header, which should contain a username and an API-key issued by the drogue-cloud API.
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
/// * `enable_api_key` - Whether to allow api keys for authentication.
///
/// # Example
///
/// ```
/// use anyhow::Result;
/// use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
/// use drogue_cloud_service_common::actix_auth::authentication::AuthN;
/// use drogue_cloud_service_common::actix_auth::authorization::AuthZ;
/// use drogue_cloud_service_api::auth::user::authz::Permission;
/// use drogue_cloud_service_common::client::UserAuthClientConfig;
/// use drogue_cloud_service_common::config::ConfigFromEnv;
/// use drogue_cloud_service_common::openid::AuthenticatorConfig;
/// use drogue_cloud_service_common::client::UserAuthClient;
/// use serde::Deserialize;
/// use reqwest;
///
/// #[derive(Clone, Debug, Deserialize)]
/// pub struct Config {
///     #[serde(default)]
///     pub user_auth: Option<UserAuthClientConfig>,
///     pub oauth: AuthenticatorConfig,
/// }
///
/// #[actix_web::main]
/// async fn main() -> Result<()> {
///
/// let config = Config::from_env()?;
/// let client = reqwest::Client::new();
///
/// let authenticator = config.oauth.into_client().await?;
/// let user_auth = if let Some(user_auth) = config.user_auth {
///     let user_auth = UserAuthClient::from_config(client.clone(), user_auth).await?;
///     Some(user_auth)
/// } else {
///     None
/// };
///
/// HttpServer::new(move || {
///     App::new()
///     .service(
///          web::scope("/index.html")
///         .wrap(AuthN {
///              openid: authenticator.as_ref().cloned(),
///              token: user_auth.clone(),
///              enable_api_key: true,
///         })
///         .service(web::resource("/").to(|| HttpResponse::Ok()))
///      )
///     })
///     .bind("127.0.0.1:8080")?
///     .run()
///     .await;
///
///     Ok(())
/// }
/// ```
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
