use crate::error::ServiceError;
use failure::_core::fmt::Formatter;
use openid::{Claims, Client, CompactJson, Discovered, IdToken, Jws};
use serde::ser::SerializeMap;
use serde::Serializer;
use std::fmt::Debug;

pub struct Authenticator {
    pub client: Option<openid::Client>,
}

impl Debug for Authenticator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("Authenticator");

        match self.client {
            None => {
                d.field("client", &"None".to_string());
            }
            Some(_) => {
                d.field("client", &"Some(...)".to_string());
            }
        }

        d.finish()
    }
}

impl Authenticator {
    pub async fn validate_token(&self, token: String) -> Result<(), actix_web::Error> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| ServiceError::InternalError {
                message: "Missing an authenticator, when performing authentication".into(),
            })?;

        let mut token = Jws::new_encoded(&token);
        match client.decode_token(&mut token) {
            Ok(_) => Ok(()),
            Err(err) => {
                log::info!("Failed to decode token: {}", err);
                Err(ServiceError::AuthenticationError)
            }
        }?;

        log::info!("Token: {:#?}", token);

        match client.validate_token(&token, None, None) {
            Ok(_) => Ok(()),
            Err(err) => {
                log::info!("Validation failed: {}", err);
                Err(ServiceError::AuthenticationError.into())
            }
        }
    }
}
