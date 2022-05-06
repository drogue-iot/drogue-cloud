use crate::client::DeviceStateClientConfig;
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct StateControllerConfiguration {
    #[serde(default)]
    pub client: DeviceStateClientConfig,
    pub endpoint: String,
    /// The amount of time to ping again before the session expiration.
    #[serde(with = "humantime_serde", default = "default_delay_buffer")]
    pub delay_buffer: Duration,
    /// The minimum delay time to wait before another ping.
    #[serde(with = "humantime_serde", default = "default_min_delay")]
    pub min_delay: Duration,
    /// Number of retries when deleting.
    #[serde(default = "default_retry_deletes")]
    pub retry_deletes: usize,
    #[serde(default = "default_retry_init")]
    pub retry_init: usize,
    #[serde(with = "humantime_serde", default)]
    pub init_delay: Option<Duration>,
}

impl Default for StateControllerConfiguration {
    fn default() -> Self {
        Self {
            client: Default::default(),
            endpoint: "default".to_string(),
            delay_buffer: default_delay_buffer(),
            min_delay: default_min_delay(),
            retry_deletes: default_retry_deletes(),
            retry_init: default_retry_init(),
            init_delay: None,
        }
    }
}

const fn default_delay_buffer() -> Duration {
    Duration::from_secs(5)
}

const fn default_min_delay() -> Duration {
    Duration::from_secs(1)
}

const fn default_retry_deletes() -> usize {
    10
}

const fn default_retry_init() -> usize {
    10
}
