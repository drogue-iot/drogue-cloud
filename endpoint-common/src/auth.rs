use crate::error::EndpointError;
use actix_web::dev::{Payload, PayloadStream};
use actix_web::{FromRequest, HttpRequest};
use anyhow::Context;
use envconfig::Envconfig;
use futures::future::{err, ok, Ready};
use reqwest::{Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::TryFrom;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Outcome {
    Pass(DeviceProperties),
    Fail,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Envconfig)]
pub struct AuthConfig {
    #[envconfig(from = "AUTH_SERVICE_URL")]
    pub auth_service_url: String,
}

#[derive(Clone, Debug)]
pub struct DeviceAuthenticator {
    client: reqwest::Client,
    auth_service_url: Url,
}

impl TryFrom<AuthConfig> for DeviceAuthenticator {
    type Error = anyhow::Error;
    fn try_from(config: AuthConfig) -> Result<Self, Self::Error> {
        let url: Url = config
            .auth_service_url
            .parse()
            .context("Failed to parse URL for auth service")?;
        let url = url
            .join("/api/v1/auth")
            .context("Failed to build auth URL from base URL")?;
        Ok(DeviceAuthenticator {
            client: Default::default(),
            auth_service_url: url,
        })
    }
}

impl DeviceAuthenticator {
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Outcome, EndpointError> {
        let response: Response = self
            .client
            .post(self.auth_service_url.clone())
            .json(&AuthRequest {
                username: username.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .map_err(|err| {
                log::warn!("Error while authenticating {}: {}", username, err);
                EndpointError::AuthenticationServiceError {
                    source: Box::new(err),
                }
            })?;

        match (response.status(), response.json::<AuthResponse>().await) {
            (StatusCode::OK, Ok(result)) => {
                log::debug!("Outcome for {} is {:?}", username, result);
                Ok(result.outcome)
            }
            (StatusCode::OK, Err(err)) => {
                log::debug!("Authentication failed for {}. Result: {:?}", username, err);

                let err = anyhow::anyhow!(
                    "Result from authentication service unexpected: OK: {:?}",
                    err
                );

                Err(EndpointError::AuthenticationServiceError {
                    source: Box::from(err),
                })
            }
            (code, _) => {
                log::debug!("Authentication failed for {}. Result: {}", username, code);

                let err =
                    anyhow::anyhow!("Result from authentication service unexpected: {}", code);

                Err(EndpointError::AuthenticationServiceError {
                    source: Box::from(err),
                })
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceProperties(pub Value);

impl FromRequest for DeviceProperties {
    type Error = ();
    type Future = Ready<Result<Self, Self::Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload<PayloadStream>) -> Self::Future {
        match req.extensions().get::<DeviceProperties>() {
            Some(properties) => ok(properties.clone()),
            None => err(()),
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use serde_json::json;

    #[test]
    fn test_encode_fail() {
        let str = serde_json::to_string(&AuthResponse {
            outcome: Outcome::Fail,
        });
        assert!(str.is_ok());
        assert_eq!(String::from(r#"{"outcome":"Fail"}"#), str.unwrap());
    }

    #[test]
    fn test_encode_pass() {
        let str = serde_json::to_string(&AuthResponse {
            outcome: Outcome::Pass(DeviceProperties(json!({"foo": "bar"}))),
        });
        assert!(str.is_ok());
        assert_eq!(
            String::from(r#"{"outcome":{"Pass":{"foo":"bar"}}}"#),
            str.unwrap()
        );
    }
}
