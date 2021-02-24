use chrono::{DateTime, TimeZone, Utc};

/// The unix epoch, as [`chrono::DateTime`].
pub fn epoch() -> DateTime<Utc> {
    Utc.timestamp(0, 0)
}
