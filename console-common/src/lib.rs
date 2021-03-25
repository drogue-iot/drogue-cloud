use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct UserInfo {
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub email: Option<String>,
}
