use std::time::{SystemTime, UNIX_EPOCH};

/// The number of milliseconds between the epoch and [`SystemTime::now`].
pub fn millis_since_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time travel is not supported")
        // epoch + (2^64)-1 milliseconds is enough time to figure out a solution for this problem
        .as_millis() as u64
}
