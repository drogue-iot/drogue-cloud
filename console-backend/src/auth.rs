use crate::error::ErrorResponse;
use crate::error::ServiceError;
use actix_web::{get, http, web, HttpResponse, Responder};
use failure::_core::fmt::Formatter;
use openid::Jws;
use serde::Deserialize;
use serde_json::json;
use std::fmt::Debug;

pub struct Authenticator {
    pub client: Option<openid::Client>,
    pub scopes: String,
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

#[get("/ui/login")]
pub async fn login(authenticator: web::Data<Authenticator>) -> impl Responder {
    if let Some(client) = authenticator.client.as_ref() {
        let auth_url = client.auth_uri(Some(&authenticator.scopes), None);

        HttpResponse::Found()
            .header(http::header::LOCATION, auth_url.to_string())
            .finish()
    } else {
        // if we are missing the authenticator, we hide ourselves
        HttpResponse::NotFound().finish()
    }
}

#[derive(Deserialize, Debug)]
pub struct LoginQuery {
    code: String,
    nonce: Option<String>,
}

#[get("/ui/token")]
pub async fn code(
    authenticator: web::Data<Authenticator>,
    query: web::Query<LoginQuery>,
) -> impl Responder {
    if let Some(client) = authenticator.client.as_ref() {
        let response = client
            .authenticate(&query.code, query.nonce.as_deref(), None)
            .await;

        log::info!(
            "Response: {:?}",
            response.as_ref().map(|r| r.bearer.clone())
        );

        match response {
            Ok(token) => HttpResponse::Ok().json(json!({ "bearer": token.bearer })),
            Err(err) => HttpResponse::Unauthorized().json(ErrorResponse {
                error: "Unauthorized".to_string(),
                message: format!("Code invalid: {:?}", err),
            }),
        }
    } else {
        // if we are missing the authenticator, we hide ourselves
        HttpResponse::NotFound().finish()
    }
}
