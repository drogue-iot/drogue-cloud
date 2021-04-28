mod authenticator;
mod config;
mod sso;
mod validate;

pub use self::config::*;
pub use authenticator::*;
pub use sso::*;

use drogue_cloud_service_api::auth::user::UserDetails;
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

impl From<ExtendedClaims> for UserDetails {
    fn from(claims: ExtendedClaims) -> Self {
        // TODO: This currently on works for Keycloak
        let roles = &claims.extended_claims["resource_access"]["services"]["roles"];
        let roles = if let Some(roles) = roles.as_array() {
            roles
                .into_iter()
                .filter_map(|v| v.as_str())
                .map(Into::into)
                .collect()
        } else {
            vec![]
        };

        Self {
            user_id: claims.standard_claims.sub,
            roles,
        }
    }
}
