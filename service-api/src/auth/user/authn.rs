use crate::auth::user::UserDetails;
use serde::{Deserialize, Serialize};

/// Authenticate a user using a password request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticationRequest {
    pub user_id: String,
    pub api_key: String,
}

/// The result of an authentication request.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthenticationResponse {
    /// The outcome, of the request.
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Outcome {
    Known(UserDetails),
    Unknown,
}
