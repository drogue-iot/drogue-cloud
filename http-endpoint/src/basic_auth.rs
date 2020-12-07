use actix_web::dev::{Payload, PayloadStream, ServiceRequest};
use actix_web::error::ErrorBadRequest;
use actix_web::http::header;
use actix_web::{Error, FromRequest, HttpMessage, HttpRequest};

use actix_web_httpauth::extractors::basic::{BasicAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;

use anyhow::Context;
use drogue_cloud_endpoint_common::error::{EndpointError, HttpEndpointError};
use envconfig::Envconfig;
use futures::future::{err, ok, Ready};
use reqwest::{Response, StatusCode, Url};
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
    pub auth_service_url: Url,
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

pub async fn basic_validator(
    req: ServiceRequest,
    cred: BasicAuth,
) -> Result<ServiceRequest, Error> {
    let authenticator = req.app_data::<DeviceAuthenticator>().ok_or_else(|| {
        HttpEndpointError(EndpointError::ConfigurationError {
            details: "Missing authentication configuration".into(),
        })
    })?;

    let config = req.app_data::<Config>();

    // We fetch the encoded header to avoid re-encoding
    let encoded_basic_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or_else(|| ErrorBadRequest("Missing Authorization header"))?;

    let response: Response = authenticator
        .client
        .get(authenticator.auth_service_url.clone())
        .header(header::AUTHORIZATION, encoded_basic_header.clone())
        .send()
        .await
        .map_err(|err| {
            log::warn!("Error while authenticating {}: {}", cred.user_id(), err);
            Error::from(AuthenticationError::from(
                config.cloned().unwrap_or_default(),
            ))
        })?;

    if response.status() == StatusCode::OK {
        log::debug!("{} authenticated successfully", cred.user_id());
        let props = response.json::<Value>().await.unwrap_or_else(|_| json!({}));
        req.extensions_mut().insert(DeviceProperties(props));
        Ok(req)
    } else {
        log::debug!(
            "Authentication failed for {}. Result: {}",
            cred.user_id(),
            response.status()
        );
        Err(AuthenticationError::from(config.cloned().unwrap_or_default()).into())
    }
}
