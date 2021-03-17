mod mock;

pub use mock::*;

use crate::{
    error::ServiceError,
    openid::{Authenticator, AuthenticatorError},
};
use actix_http::{Payload, PayloadStream};
use actix_web::dev::ServiceRequest;
use actix_web::{FromRequest, HttpMessage, HttpRequest};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use openid::StandardClaims;
use std::future::{ready, Ready};

#[derive(Clone, Debug)]
pub enum UserInformation {
    Authenticated(StandardClaims),
    Anonymous,
}

/// An identity, might also be anonymous.
pub trait Identity: Send + Sync {
    /// The ID of the user, or [`None`] if it is an anonymous identity.
    fn user_id(&self) -> Option<&str>;
}

impl Identity for UserInformation {
    fn user_id(&self) -> Option<&str> {
        match self {
            Self::Anonymous => None,
            Self::Authenticated(claims) => Some(&claims.sub),
        }
    }
}

impl FromRequest for UserInformation {
    type Config = ();
    type Error = ();
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload<PayloadStream>) -> Self::Future {
        match req.extensions().get::<UserInformation>() {
            Some(user) => ready(Ok(user.clone())),
            None => ready(Ok(UserInformation::Anonymous)),
        }
    }
}

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
        Ok(token) => match token.payload() {
            Ok(payload) => {
                req.extensions_mut()
                    .insert(UserInformation::Authenticated(payload.clone()));
                Ok(req)
            }
            Err(err) => {
                log::debug!("Failed to extract token payload: {}", err);
                Err(ServiceError::AuthenticationError.into())
            }
        },
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
        actix_web_httpauth::middleware::HttpAuthentication::bearer(|req, auth| $crate::auth::openid_validator(req, auth, |$req| $($extract)*))
    };
}
