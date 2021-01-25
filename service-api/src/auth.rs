use super::management;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticationRequest {
    pub tenant: String,
    pub device: String,
    pub credential: Credential,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Credential {
    #[serde(rename = "user")]
    UsernamePassword { username: String, password: String },
    #[serde(rename = "pass")]
    Password(String),
    #[serde(rename = "cert")]
    Certificate(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass {
        tenant: management::Tenant,
        device: management::Device,
    },
    Fail,
}

#[derive(thiserror::Error, Debug)]
pub enum AuthenticationClientError<E: 'static>
where
    E: std::error::Error,
{
    #[error("client error: {0}")]
    Client(#[from] Box<E>),
    #[error("request error: {0}")]
    Request(String),
    #[error("service error: {0}")]
    Service(ErrorInformation),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorInformation {
    pub error: String,
    #[serde(default)]
    pub message: String,
}

impl fmt::Display for ErrorInformation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error, self.message)
    }
}

#[async_trait]
pub trait AuthenticationClient {
    type Error: std::error::Error;
    async fn authenticate(
        &self,
        request: AuthenticationRequest,
    ) -> Result<AuthenticationResponse, AuthenticationClientError<Self::Error>>;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthenticationResponse {
    pub outcome: Outcome,
}

impl AuthenticationResponse {
    pub fn failed() -> Self {
        Self {
            outcome: Outcome::Fail,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
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

    #[test]
    fn test_encode_fail() {
        let str = serde_json::to_string(&AuthenticationResponse {
            outcome: Outcome::Fail,
        });
        assert!(str.is_ok());
        assert_eq!(String::from(r#"{"outcome":"fail"}"#), str.unwrap());
    }

    #[test]
    fn test_encode_pass() {
        let str = serde_json::to_string(&AuthenticationResponse {
            outcome: Outcome::Pass {
                tenant: management::Tenant {
                    id: "t1".to_string(),
                    data: Default::default(),
                },
                device: management::Device {
                    tenant_id: "t1".to_string(),
                    id: "d1".to_string(),
                    data: Default::default(),
                },
            },
        });

        assert!(str.is_ok());
        assert_eq!(
            String::from(
                r#"{"outcome":{"pass":{"tenant":{"id":"t1","data":{}},"device":{"tenant_id":"t1","id":"d1","data":{}}}}}"#
            ),
            str.unwrap()
        );
    }
}
