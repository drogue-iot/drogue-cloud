mod authenticator;
mod config;
mod sso;

pub use self::config::*;
pub use authenticator::*;
pub use sso::*;

use openid::{CompactJson, CustomClaims, StandardClaims};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtendedClaims {
    #[serde(flatten)]
    standard_claims: StandardClaims,
    #[serde(flatten)]
    pub extended_claims: serde_json::Value,
}

impl CustomClaims for ExtendedClaims {
    fn standard_claims(&self) -> &StandardClaims {
        &self.standard_claims
    }
}

impl CompactJson for ExtendedClaims {}
