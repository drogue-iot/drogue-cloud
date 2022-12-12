use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use drogue_client::tokens::v1::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessTokenData {
    pub hashed_token: String,
    pub created: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub claims: Option<AccessTokenClaims>,
}
