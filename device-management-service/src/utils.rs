use chrono::{DateTime, TimeZone, Utc};

pub fn epoch() -> DateTime<Utc> {
    Utc.timestamp(0, 0)
}
