mod authenticator;
mod provider;

pub use authenticator::*;
pub use provider::*;

pub trait Expires {
    /// Check if the resources expires before the duration elapsed.
    fn expires_before(&self, duration: chrono::Duration) -> bool {
        match self.expires_in() {
            Some(expires) => expires >= duration,
            None => false,
        }
    }

    /// Get the duration until this resource expires. This may be negative.
    fn expires_in(&self) -> Option<chrono::Duration> {
        self.expires().map(|expires| expires - chrono::Utc::now())
    }

    /// Get the timestamp when the resource expires.
    fn expires(&self) -> Option<chrono::DateTime<chrono::Utc>>;
}

impl Expires for openid::Bearer {
    fn expires(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.expires
    }
}
