use crate::x509::ClientCertificateChain;
use actix_web::{
    dev::{Payload, PayloadStream},
    {FromRequest, HttpRequest},
};
use anyhow::Context;
use drogue_cloud_service_api::{
    auth::{
        authn::{AuthenticationRequest, AuthenticationResponse, Credential},
        ClientError,
    },
    management::{Application, Device},
};
use drogue_cloud_service_common::{
    client::ReqwestAuthenticatorClient,
    config::ConfigFromEnv,
    openid::{OpenIdTokenProvider, TokenConfig},
};
use envconfig::Envconfig;
use futures::future::{err, ok, Ready};
use http::HeaderValue;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use x509_parser::prelude::X509Certificate;

#[derive(Clone, Debug, Envconfig)]
pub struct AuthConfig {
    /// Disable authenticating towards the authentication service.
    #[envconfig(from = "AUTH_CLIENT_DISABLE_AUTH", default = "false")]
    pub auth_disabled: bool,

    /// The URL of the authentication service.
    #[envconfig(from = "AUTH_SERVICE_URL")]
    pub auth_service_url: String,
}

#[derive(Clone, Debug)]
pub struct DeviceAuthenticator {
    pub client: ReqwestAuthenticatorClient,
}

pub type AuthResult<T> = Result<T, ClientError<reqwest::Error>>;

impl DeviceAuthenticator {
    /// Create a new authentication client using the provided configuration.
    ///
    /// If the configuration has authentication enabled, but no token configuration is provided, an
    /// error will be returned.
    pub async fn with_config(
        config: AuthConfig,
        token_config: Option<TokenConfig>,
    ) -> anyhow::Result<Self> {
        let url: Url = config
            .auth_service_url
            .parse()
            .context("Failed to parse URL for auth service")?;
        let url = url
            .join("/api/v1/auth")
            .context("Failed to build auth URL from base URL")?;

        let token_provider = match (config.auth_disabled, token_config) {
            (false, Some(token_config)) => Some(
                OpenIdTokenProvider::discover_from(reqwest::Client::new(), token_config)
                    .await
                    .context("Failed to discover OAuth2 client")?,
            ),
            (false, None) => None,
            (true, None) => {
                anyhow::bail!("Requested OAuth2 authentication without providing a configuration")
            }
            (true, Some(_)) => {
                anyhow::bail!("Provided an OAuth2 configuration without requesting authentication")
            }
        };

        Ok(DeviceAuthenticator {
            client: ReqwestAuthenticatorClient::new(Default::default(), url, token_provider),
        })
    }

    /// Create a new authentication client by using configuration from the environment.
    pub async fn new() -> anyhow::Result<Self> {
        let config: AuthConfig = AuthConfig::init_from_env()?;

        let token_config = match config.auth_disabled {
            false => Some(TokenConfig::from_env()?),
            true => None,
        };

        Self::with_config(config, token_config).await
    }

    // FIXME: check why this still exists
    /// authenticate with a combination of `<device>@<tenant>` / `<password>`.
    pub async fn x_authenticate_simple(
        &self,
        device: &str,
        password: &str,
    ) -> AuthResult<AuthenticationResponse> {
        let tok: Vec<_> = device
            .split('@')
            .map(|s| percent_encoding::percent_decode_str(s).decode_utf8())
            .collect();

        match (
            tok.as_slice(),
            percent_encoding::percent_decode_str(password).decode_utf8(),
        ) {
            ([Ok(device), Ok(tenant)], Ok(password)) => {
                self.authenticate(
                    tenant,
                    device,
                    Credential::Password(password.to_string()),
                    None,
                )
                .await
            }
            _ => Ok(AuthenticationResponse::failed()),
        }
    }

    pub async fn authenticate<A, D>(
        &self,
        application: A,
        device: D,
        credential: Credential,
        r#as: Option<String>,
    ) -> AuthResult<AuthenticationResponse>
    where
        A: ToString,
        D: ToString,
    {
        self.client
            .authenticate(AuthenticationRequest {
                application: application.to_string(),
                device: device.to_string(),
                credential,
                r#as,
            })
            .await
    }

    /// Authenticate a device from a client cert only.
    ///
    /// This will take the issuerDn as application id, and the subjectDn as device id.
    pub async fn authenticate_cert(
        &self,
        certs: Vec<Vec<u8>>,
    ) -> AuthResult<AuthenticationResponse> {
        let (app_id, device_id) = Self::ids_from_cert(&certs)?;
        self.authenticate(app_id, device_id, Credential::Certificate(certs), None)
            .await
    }

    /// authenticate for a typical MQTT request
    pub async fn authenticate_mqtt<U, P, C>(
        &self,
        username: Option<U>,
        password: Option<P>,
        client_id: C,
        certs: Option<ClientCertificateChain>,
    ) -> AuthResult<AuthenticationResponse>
    where
        U: AsRef<str> + Debug,
        P: Into<String> + Debug,
        C: AsRef<str> + Debug,
    {
        log::debug!(
            "Authenticate MQTT - username: {:?}, password: {:?}, client_id: {:?}, certs: {:?}",
            username,
            password,
            client_id,
            certs
        );

        match (
            username.map(Username::from),
            password,
            Username::from(client_id),
            certs,
        ) {
            // Username/password <device>@<tenant> / <password>, Client ID: ???
            (Some(Username::Scoped { scope, device }), Some(password), _, None) => {
                self.authenticate(&scope, &device, Credential::Password(password.into()), None)
                    .await
            }
            // Username/password <username> / <password>, Client ID: <device>@<tenant>
            (
                Some(Username::NonScoped(username)),
                Some(password),
                Username::Scoped { scope, device },
                None,
            ) => {
                self.authenticate(
                    &scope,
                    &device,
                    Credential::UsernamePassword {
                        username,
                        password: password.into(),
                    },
                    None,
                )
                .await
            }
            // Client cert only
            (None, None, _, Some(certs)) => self.authenticate_cert(certs.0).await,
            // everything else is failed
            _ => Ok(AuthenticationResponse::failed()),
        }
    }

    pub fn ids_from_cert(certs: &[Vec<u8>]) -> AuthResult<(String, String)> {
        let cert = Self::device_cert(&certs)?;
        let app_id = cert.tbs_certificate.issuer.to_string();
        let device_id = cert.tbs_certificate.subject.to_string();
        Ok((app_id, device_id))
    }

    /// authenticate for a typical HTTP request
    pub async fn authenticate_http<T, D>(
        &self,
        tenant: Option<T>,
        device: Option<D>,
        auth: Option<&HeaderValue>,
        certs: Option<Vec<Vec<u8>>>,
        r#as: Option<String>,
    ) -> AuthResult<AuthenticationResponse>
    where
        T: AsRef<str>,
        D: AsRef<str>,
    {
        match (tenant, device, auth.map(AuthValue::from), certs) {
            // POST /<channel> -> basic auth `<device>@<tenant>` / `<password>` -> Password(<password>)
            (
                None,
                None,
                Some(AuthValue::Basic {
                    username: Username::Scoped { scope, device },
                    password,
                }),
                None,
            ) => {
                self.authenticate(&scope, &device, Credential::Password(password), r#as)
                    .await
            }
            // POST /<channel>?tenant=<tenant> -> basic auth `<device>` / `<password>` -> Password(<password>)
            (Some(scope), None, Some(AuthValue::Basic { username, password }), None) => {
                self.authenticate(
                    scope.as_ref(),
                    username.into_string(),
                    Credential::Password(password),
                    r#as,
                )
                .await
            }
            // POST /<channel>?tenant=<tenant>&device=<device> -> basic auth `<username>` / `<password>` -> UsernamePassword(<username>, <password>)
            (Some(scope), Some(device), Some(AuthValue::Basic { username, password }), None) => {
                self.authenticate(
                    scope.as_ref(),
                    device.as_ref(),
                    Credential::UsernamePassword {
                        username: username.into_string(),
                        password,
                    },
                    r#as,
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
                None,
            ) => {
                self.authenticate(
                    &scope,
                    device.as_ref(),
                    Credential::UsernamePassword { username, password },
                    r#as,
                )
                .await
            }

            // X.509 client certificate -> all information from the cert
            (None, None, None, Some(certs)) => self.authenticate_cert(certs).await,

            // everything else is failed
            _ => Ok(AuthenticationResponse::failed()),
        }
    }

    /// Retrieve the end-entity (aka device) certificate, must be the first one.
    fn device_cert(certs: &[Vec<u8>]) -> AuthResult<X509Certificate> {
        match certs.get(0) {
            Some(cert) => Ok(x509_parser::parse_x509_certificate(&cert)
                .map_err(|err| {
                    ClientError::Request(format!("Failed to parse client certificate: {}", err))
                })?
                .1),
            None => Err(ClientError::Request(
                "Empty client certificate chain".into(),
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceAuthDetails {
    pub app: Application,
    pub device: Device,
}

impl FromRequest for DeviceAuthDetails {
    type Config = ();
    type Error = ();
    type Future = Ready<Result<Self, Self::Error>>;

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
