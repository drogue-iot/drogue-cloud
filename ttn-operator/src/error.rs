use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ReconcileError {
    #[error("Reconciliation failed with a permanent error: {0}")]
    Permanent(String),
    #[error("Reconciliation failed with a temporary error: {0}")]
    Temporary(String),
}

impl ReconcileError {
    pub fn permanent<S: ToString>(s: S) -> Self {
        Self::Permanent(s.to_string())
    }
    pub fn temporary<S: ToString>(s: S) -> Self {
        Self::Temporary(s.to_string())
    }
}

impl From<reqwest::Error> for ReconcileError {
    fn from(err: reqwest::Error) -> Self {
        Self::permanent(err)
    }
}

impl From<serde_json::Error> for ReconcileError {
    fn from(err: serde_json::Error) -> Self {
        Self::permanent(err)
    }
}

pub trait ToPermanent {
    fn perm(self) -> ReconcileError;
}

impl<E: std::error::Error> ToPermanent for E {
    fn perm(self) -> ReconcileError {
        ReconcileError::Permanent(self.to_string())
    }
}
