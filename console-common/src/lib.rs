use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub account_url: Option<String>,
}
