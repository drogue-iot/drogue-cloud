use crate::auth::user::UserDetails;
use crate::metrics::{AsPassFail, PassFail};
use serde::{Deserialize, Serialize};

/// Authenticate a user using a password request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthenticationRequest {
    pub user_id: String,
    pub access_token: String,
}

/// The outcome of an authentication request.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Outcome {
    Known(UserDetails),
    Unknown,
}

/// The result of an authentication request.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthenticationResponse {
    /// The outcome, of the request.
    pub outcome: Outcome,
}

impl AsPassFail for AuthenticationResponse {
    fn as_pass_fail(&self) -> PassFail {
        match self.outcome {
            Outcome::Known(_) => PassFail::Pass,
            Outcome::Unknown => PassFail::Fail,
        }
    }
}
