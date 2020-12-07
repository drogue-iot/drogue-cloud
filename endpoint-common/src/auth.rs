use crate::error::EndpointError;
use actix_web::dev::{Payload, PayloadStream};
use actix_web::{FromRequest, HttpRequest};
use anyhow::Context;
use envconfig::Envconfig;
use futures::future::{err, ok, Ready};
use headers::authorization::Credentials;
use reqwest::{header, Response, StatusCode, Url};
use serde_json::{json, Value};
use std::convert::TryFrom;

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

pub enum Outcome {
    Pass(DeviceProperties),
    Fail,
}

impl DeviceAuthenticator {
    pub async fn authenticate(
        &self,
        device_id: &str,
        password: &str,
    ) -> Result<Outcome, EndpointError> {
        let auth = headers::Authorization::basic(device_id, password);

        let response: Response = self
            .client
            .get(self.auth_service_url.clone())
            .header(header::AUTHORIZATION, auth.0.encode())
            .send()
            .await
            .map_err(|err| {
                log::warn!("Error while authenticating {}: {}", device_id, err);
                EndpointError::AuthenticationServiceError {
                    source: Box::new(err),
                }
            })?;

        match response.status() {
            StatusCode::OK => {
                log::debug!("{} authenticated successfully", device_id);
                let props = response.json::<Value>().await.unwrap_or_else(|_| json!({}));
                Ok(Outcome::Pass(DeviceProperties(props)))
            }
            StatusCode::FORBIDDEN => {
                // FIXME: Right now this is the result when the device could not get authenticated.
                // However, it could also mean we are not allowed to access the service.
                Ok(Outcome::Fail)
            }
            code => {
                log::debug!(
                    "Authentication failed for {}. Result: {}",
                    device_id,
                    response.status()
                );

                let err =
                    anyhow::anyhow!("Result from authentication service unexpected: {}", code);

                Err(EndpointError::AuthenticationServiceError {
                    source: Box::from(err),
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
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
