use core::fmt::{self, Formatter};
use drogue_client::registry;
use serde::{Deserialize, Serialize};

/// Authenticate a device.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticationRequest {
    pub application: String,
    pub device: String,
    pub credential: Credential,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#as: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Credential {
    #[serde(rename = "user")]
    UsernamePassword { username: String, password: String },
    #[serde(rename = "pass")]
    Password(String),
    #[serde(rename = "cert")]
    Certificate(Vec<Vec<u8>>),
}

struct Ellipsis;

impl fmt::Debug for Ellipsis {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("...")
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password(_) => f.debug_tuple("Password").field(&Ellipsis).finish(),
            Self::UsernamePassword { username, .. } => f
                .debug_struct("UsernamePassword")
                .field("username", username)
                .field("password", &Ellipsis)
                .finish(),
            Self::Certificate(_) => f.debug_tuple("Certificate").field(&Ellipsis).finish(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The authentication request passed. The outcome also contains application and device
    /// details for further processing.
    Pass {
        application: registry::v1::Application,
        device: registry::v1::Device,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#as: Option<registry::v1::Device>,
    },
    /// The authentication request failed. The device is not authenticated, and the device's
    /// request must be rejected.
    Fail,
}

/// The result of an authentication request.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthenticationResponse {
    /// The outcome, if the request.
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
    use chrono::{TimeZone, Utc};
    use drogue_client::meta;
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
                application: registry::v1::Application {
                    metadata: meta::v1::NonScopedMetadata {
                        name: "a1".to_string(),
                        creation_timestamp: Utc.timestamp_millis(1000),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                r#as: None,
                device: registry::v1::Device {
                    metadata: meta::v1::ScopedMetadata {
                        application: "a1".to_string(),
                        name: "d1".to_string(),
                        creation_timestamp: Utc.timestamp_millis(1234),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        });

        assert!(str.is_ok());
        assert_eq!(
            String::from(
                r#"{"outcome":{"pass":{"application":{"metadata":{"name":"a1","creationTimestamp":"1970-01-01T00:00:01Z","generation":0}},"device":{"metadata":{"application":"a1","name":"d1","creationTimestamp":"1970-01-01T00:00:01.234Z","generation":0}}}}}"#
            ),
            str.unwrap()
        );
    }

    #[test]
    fn test_no_leak_password() {
        assert_eq!(
            "Password(...)",
            format!("{:?}", Credential::Password("foo".into()))
        );
    }

    #[test]
    fn test_no_leak_username_password() {
        assert_eq!(
            r#"UsernamePassword { username: "foo", password: ... }"#,
            format!(
                "{:?}",
                Credential::UsernamePassword {
                    username: "foo".into(),
                    password: "bar".into()
                }
            )
        );
    }
}
