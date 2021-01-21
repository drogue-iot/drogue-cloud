use serde::{Deserialize, Serialize};
use std::fmt;

const fn fn_true() -> bool {
    true
}
fn is_true(b: &bool) -> bool {
    *b
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,

    pub data: TenantData,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TenantData {
    #[serde(default = "fn_true")]
    #[serde(skip_serializing_if = "is_true")]
    pub enabled: bool,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Device {
    pub tenant_id: String,
    pub id: String,
    pub data: DeviceData,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct DeviceData {
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<Credential>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticationRequest {
    pub tenant: String,
    pub device: String,
    pub credential: Credential,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Credential {
    #[serde(rename = "user")]
    UsernamePassword {
        username: String,
        password: String,
        #[serde(default)]
        unique: bool,
    },
    #[serde(rename = "pass")]
    Password(String),
    #[serde(rename = "cert")]
    Certificate(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Outcome {
    #[serde(rename = "pass")]
    Pass { tenant: Tenant, device: Device },
    #[serde(rename = "fail")]
    Fail,
}

#[derive(thiserror::Error, Debug)]
pub enum AuthenticationClientError {
    #[error("service error: {0}")]
    Service(ErrorInformation),
}

#[derive(Clone, Debug)]
pub struct ErrorInformation {
    pub error: String,
    pub message: String,
}

impl fmt::Display for ErrorInformation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error, self.message)
    }
}

pub trait AuthenticationClient {
    fn authenticate(
        &self,
        request: AuthenticationRequest,
    ) -> Result<Outcome, AuthenticationClientError>;
}

#[cfg(test)]
mod test {
    use crate::Credential;
    use serde_json::json;

    #[test]
    fn ser_credentials() {
        let ser = serde_json::to_value(vec![
            Credential::Password("foo".into()),
            Credential::UsernamePassword {
                username: "foo".into(),
                password: "bar".into(),
            },
        ]);
        assert!(ser.is_ok());
        assert_eq!(
            ser.unwrap(),
            json! {[
                {"pass": "foo"},
                {"user": {"username": "foo", "password": "bar"}}
            ]}
        )
    }
}
