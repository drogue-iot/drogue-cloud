use drogue_cloud_service_common::defaults;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
pub struct KafkaConfig {
    #[serde(default = "defaults::kafka_bootstrap_servers")]
    pub bootstrap_servers: String,
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: defaults::kafka_bootstrap_servers(),
            custom: Default::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use drogue_cloud_service_common::config::ConfigFromEnv;

    #[test]
    fn test_custom() {
        std::env::set_var("KAFKA__CUSTOM__A_B_C", "d.e.f");

        let kafka = KafkaConfig::from_env_prefix("KAFKA").unwrap();

        assert_eq!(kafka.custom.get("a_b_c").cloned(), Some("d.e.f".into()));

        std::env::remove_var("KAFKA__CUSTOM__A_B_C");
    }
}
