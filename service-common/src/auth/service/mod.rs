mod mock;

pub use mock::*;

use crate::error::ServiceError;
use crate::openid::{Authenticator, AuthenticatorError};
use actix_web::dev::ServiceRequest;
use actix_web_httpauth::extractors::bearer::BearerAuth;

pub async fn openid_validator<F>(
    req: ServiceRequest,
    auth: BearerAuth,
    extract: F,
) -> Result<ServiceRequest, actix_web::Error>
where
    F: Fn(&ServiceRequest) -> Option<&Authenticator>,
{
    let token = auth.token().to_string();

    let authenticator = extract(&req);
    log::debug!("Authenticator: {:?}", &authenticator);
    let authenticator = authenticator.ok_or_else(|| ServiceError::InternalError {
        message: "Missing authenticator instance".into(),
    })?;

    match authenticator.validate_token(token).await {
        Ok(_) => Ok(req),
        Err(AuthenticatorError::Missing) => Err(ServiceError::InternalError {
            message: "Missing OpenID client".into(),
        }
        .into()),
        Err(AuthenticatorError::Failed) => Err(ServiceError::AuthenticationError.into()),
    }
}

#[macro_export]
macro_rules! openid_auth {
    ($req:ident -> $($extract:tt)* ) => {
        HttpAuthentication::bearer(|req, auth| $crate::auth::openid_validator(req, auth, |$req| $($extract)*))
    };
}
