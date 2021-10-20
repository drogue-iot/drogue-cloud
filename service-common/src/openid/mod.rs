mod authenticator;
mod config;
mod validate;

pub use self::config::*;
pub use authenticator::*;

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
        let mut roles = Vec::new();

        // realm access

        let r = &claims.extended_claims["realm_access"]["roles"];
        if let Some(r) = r.as_array() {
            roles.extend(r.iter().filter_map(|v| v.as_str()).map(Into::into));
        }

        for client in ["services", "drogue"] {
            let r = &claims.extended_claims["resource_access"][client]["roles"];
            if let Some(r) = r.as_array() {
                roles.extend(r.iter().filter_map(|v| v.as_str()).map(Into::into));
            }
        }

        Self {
            user_id: claims.standard_claims.sub,
            roles,
        }
    }
}
