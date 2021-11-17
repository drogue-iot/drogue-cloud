pub mod app;

use serde::Deserialize;
use std::{collections::HashMap, num::NonZeroU32};

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerConfig {
    #[serde(default = "default::num_partitions")]
    pub num_partitions: NonZeroU32,
    #[serde(default = "default::num_replicas")]
    pub num_replicas: NonZeroU32,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

impl ControllerConfig {
    /// Translate the configuration from env-var style keys (with underscore) to Kafka style keys (with dots).
    pub fn translate(self) -> Self {
        let properties = self
            .properties
            .into_iter()
            .map(|(k, v)| (k.replace('_', "."), v))
            .collect();
        Self { properties, ..self }
    }
}

mod default {
    use std::num::NonZeroU32;

    const ONE: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(1) };

    pub(crate) const fn num_partitions() -> NonZeroU32 {
        ONE
    }

    pub(crate) const fn num_replicas() -> NonZeroU32 {
        ONE
    }
}
