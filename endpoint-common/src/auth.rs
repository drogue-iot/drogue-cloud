use actix_web::dev::{Payload, PayloadStream};
use actix_web::{FromRequest, HttpRequest};
use anyhow::Context;
use drogue_cloud_service_api::{
    auth::{
        AuthenticationClient, AuthenticationClientError, AuthenticationRequest,
        AuthenticationResponse, Credential,
    },
    management::{Application, Device},
};
use drogue_cloud_service_common::auth::ReqwestAuthenticatorClient;
use envconfig::Envconfig;
use futures::future::{err, ok, Ready};
use http::HeaderValue;
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
    pub client: ReqwestAuthenticatorClient,
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
            client: ReqwestAuthenticatorClient::new(Default::default(), url, None),
        })
    }
}

impl DeviceAuthenticator {
    /// authenticate with a combination of `<device>@<tenant>` / `<password>`.
    pub async fn x_authenticate_simple(
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

    pub async fn authenticate<T, D>(
        &self,
        tenant: T,
        device: D,
        credential: Credential,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<reqwest::Error>>
    where
        T: ToString,
        D: ToString,
    {
        self.client
            .authenticate(AuthenticationRequest {
                tenant: tenant.to_string(),
                device: device.to_string(),
                credential,
            })
            .await
    }

    /// authenticate for a typical MQTT request
    pub async fn authenticate_mqtt<U, P, C>(
        &self,
        username: Option<U>,
        password: Option<P>,
        client_id: C,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<reqwest::Error>>
    where
        U: AsRef<str>,
        P: Into<String>,
        C: AsRef<str>,
    {
        match (
            username.map(Username::from),
            password,
            Username::from(client_id),
        ) {
            // Username/password <device>@<tenant> / <password>, Client ID: ???
            (Some(Username::Scoped { scope, device }), Some(password), _) => {
                self.authenticate(&scope, &device, Credential::Password(password.into()))
                    .await
            }
            // Username/password <username> / <password>, Client ID: <device>@<tenant>
            (
                Some(Username::NonScoped(username)),
                Some(password),
                Username::Scoped { scope, device },
            ) => {
                self.authenticate(
                    &scope,
                    &device,
                    Credential::UsernamePassword {
                        username,
                        password: password.into(),
                    },
                )
                .await
            }
            // everything else is failed
            _ => Ok(AuthenticationResponse::failed()),
        }
    }

    /// authenticate for a typical HTTP request
    pub async fn authenticate_http<T, D>(
        &self,
        tenant: Option<T>,
        device: Option<D>,
        auth: Option<&HeaderValue>,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<reqwest::Error>>
    where
        T: AsRef<str>,
        D: AsRef<str>,
    {
        match (tenant, device, auth.map(AuthValue::from)) {
            // POST /<channel> -> basic auth `<device>@<tenant>` / `<password>` -> Password(<password>)
            (
                None,
                None,
                Some(AuthValue::Basic {
                    username: Username::Scoped { scope, device },
                    password,
                }),
            ) => {
                self.authenticate(&scope, &device, Credential::Password(password))
                    .await
            }
            // POST /<channel>?tenant=<tenant> -> basic auth `<device>` / `<password>` -> Password(<password>)
            (Some(scope), None, Some(AuthValue::Basic { username, password })) => {
                self.authenticate(
                    scope.as_ref(),
                    username.into_string(),
                    Credential::Password(password),
                )
                .await
            }
            // POST /<channel>?tenant=<tenant>&device=<device> -> basic auth `<username>` / `<password>` -> UsernamePassword(<username>, <password>)
            (Some(scope), Some(device), Some(AuthValue::Basic { username, password })) => {
                self.authenticate(
                    scope.as_ref(),
                    device.as_ref(),
                    Credential::UsernamePassword {
                        username: username.into_string(),
                        password,
                    },
                )
                .await
            }
            // POST /<channel>?device=<device> -> basic auth `<username>@<tenant>` / `<password>` -> UsernamePassword(<username>, <password>)
            (
                None,
                Some(device),
                Some(AuthValue::Basic {
                    username:
                        Username::Scoped {
                            scope,
                            device: username,
                        },
                    password,
                }),
            ) => {
                self.authenticate(
                    &scope,
                    device.as_ref(),
                    Credential::UsernamePassword { username, password },
                )
                .await
            }

            // everything else is failed
            _ => Ok(AuthenticationResponse::failed()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceAuthDetails {
    pub app: Application,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Username {
    Scoped { scope: String, device: String },
    NonScoped(String),
}

impl<S: AsRef<str>> From<S> for Username {
    fn from(s: S) -> Self {
        let s = s.as_ref();
        match s.splitn(2, '@').collect::<Vec<_>>().as_slice() {
            [device, scope] => {
                let device = percent_encoding::percent_decode_str(device).decode_utf8_lossy();
                Username::Scoped {
                    scope: scope.to_string(),
                    device: device.to_string(),
                }
            }
            _ => Username::NonScoped(s.to_string()),
        }
    }
}

impl Username {
    pub fn into_string(self) -> String {
        match self {
            Username::NonScoped(s) => s,
            Username::Scoped { scope, device } => format!("{}@{}", scope, device),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthValue {
    Basic {
        username: Username,
        password: String,
    },
    Bearer(String),
    Unknown,
}

impl From<&HeaderValue> for AuthValue {
    fn from(header: &HeaderValue) -> Self {
        let value = match header.to_str() {
            Err(_) => return AuthValue::Unknown,
            Ok(value) => value,
        };

        match value.splitn(2, ' ').collect::<Vec<_>>().as_slice() {
            ["Basic", v] => match base64::decode(v).map(String::from_utf8) {
                Ok(Ok(v)) => match v.splitn(2, ':').collect::<Vec<_>>().as_slice() {
                    [username, password] => AuthValue::Basic {
                        username: username.into(),
                        password: password.to_string(),
                    },
                    _ => AuthValue::Unknown,
                },
                _ => AuthValue::Unknown,
            },
            ["Bearer", token] => AuthValue::Bearer(token.to_string()),
            _ => AuthValue::Unknown,
        }
    }
}

impl From<HeaderValue> for AuthValue {
    fn from(header: HeaderValue) -> Self {
        AuthValue::from(&header)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_user_scoped() {
        let user = Username::from("device@scope");
        assert_eq!(
            user,
            Username::Scoped {
                scope: "scope".into(),
                device: "device".into()
            }
        )
    }

    #[test]
    fn test_basic_rfc() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static(
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==",
        ));
        assert_eq!(
            auth,
            AuthValue::Basic {
                username: "Aladdin".into(),
                password: "open sesame".into()
            }
        )
    }

    #[test]
    fn test_basic_special_username() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static("Basic Zm9vQGJhcjpiYXo="));
        assert_eq!(
            auth,
            AuthValue::Basic {
                username: "foo@bar".into(),
                password: "baz".into()
            }
        )
    }

    #[test]
    fn test_basic_invalid_base64() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static("Basic 1234"));
        assert_eq!(auth, AuthValue::Unknown)
    }

    #[test]
    fn test_basic_missing_colon() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static("Basic Zm9vYmFy"));
        assert_eq!(auth, AuthValue::Unknown)
    }

    #[test]
    fn test_unknown_scheme() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static("Foo Bar"));
        assert_eq!(auth, AuthValue::Unknown)
    }

    #[test]
    fn test_unknown_format() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static("FooBarBaz"));
        assert_eq!(auth, AuthValue::Unknown)
    }

    #[test]
    fn test_unknown_empty() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static(""));
        assert_eq!(auth, AuthValue::Unknown)
    }

    #[test]
    fn test_bearer_rfc() {
        let auth: AuthValue = AuthValue::from(HeaderValue::from_static("Bearer mF_9.B5f-4.1JqM"));
        assert_eq!(auth, AuthValue::Bearer("mF_9.B5f-4.1JqM".into()))
    }
}
