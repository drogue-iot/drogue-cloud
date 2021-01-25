use actix_web::dev::{Payload, PayloadStream};
use actix_web::{FromRequest, HttpRequest};
use anyhow::Context;
use drogue_cloud_service_api::{
    auth::{
        AuthenticationClient, AuthenticationClientError, AuthenticationRequest,
        AuthenticationResponse, Credential,
    },
    management::{Device, Tenant},
};
use drogue_cloud_service_common::auth::ReqwestAuthenticatorClient;
use envconfig::Envconfig;
use futures::future::{err, ok, Ready};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Clone, Debug, Envconfig)]
pub struct AuthConfig {
    #[envconfig(from = "AUTH_SERVICE_URL")]
    pub auth_service_url: String,
}

#[derive(Clone, Debug)]
pub struct DeviceAuthenticator {
    client: ReqwestAuthenticatorClient,
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
            client: ReqwestAuthenticatorClient::new(Default::default(), url),
        })
    }
}

impl DeviceAuthenticator {
    /// authenticate with a combination of `<device>@<tenant>` / `<password>`.
    pub async fn authenticate_simple(
        &self,
        device: &str,
        password: &str,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<reqwest::Error>> {
        let tok: Vec<_> = device
            .split('@')
            .map(|s| percent_encoding::percent_decode_str(s).decode_utf8())
            .collect();

        match (
            tok.as_slice(),
            percent_encoding::percent_decode_str(password).decode_utf8(),
        ) {
            ([Ok(device), Ok(tenant)], Ok(password)) => {
                self.authenticate(tenant, device, Credential::Password(password.to_string()))
                    .await
            }
            _ => Ok(AuthenticationResponse::failed()),
        }
    }

    pub async fn authenticate(
        &self,
        tenant: &str,
        device: &str,
        credential: Credential,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<reqwest::Error>> {
        self.client
            .authenticate(AuthenticationRequest {
                tenant: tenant.to_string(),
                device: device.to_string(),
                credential,
            })
            .await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceAuthDetails {
    pub tenant: Tenant,
    pub device: Device,
}

impl FromRequest for DeviceAuthDetails {
    type Error = ();
    type Future = Ready<Result<Self, Self::Error>>;
    type Config = ();

    fn from_request(req: &HttpRequest, _: &mut Payload<PayloadStream>) -> Self::Future {
        match req.extensions().get::<DeviceAuthDetails>() {
            Some(properties) => ok(properties.clone()),
            None => err(()),
        }
    }
}
