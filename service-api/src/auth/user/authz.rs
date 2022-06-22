use crate::metrics::{AsPassFail, PassFail};
use core::fmt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Owner,
    Admin,
    Write,
    Read,
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Authorize a request for a user.
///
/// NOTE: The user_id and roles information must come from a trusted source, like
/// a validated token. The user service will not re-validate this information.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub application: String,
    pub permission: Permission,

    pub user_id: Option<String>,
    pub roles: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Allow,
    Deny,
}

impl Outcome {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn ensure<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> E,
    {
        match self.is_allowed() {
            true => Ok(()),
            false => Err(f()),
        }
    }
}

/// The result of an authorization request.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthorizationResponse {
    /// The outcome, of the request.
    pub outcome: Outcome,
}

impl AsPassFail for AuthorizationResponse {
    fn as_pass_fail(&self) -> PassFail {
        match self.outcome {
            Outcome::Allow => PassFail::Pass,
            Outcome::Deny => PassFail::Fail,
        }
    }
}
