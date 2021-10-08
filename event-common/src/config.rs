use rdkafka::ClientConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::ops::Deref;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct KafkaClientConfig {
    #[serde(default = "kafka_bootstrap_servers")]
    pub bootstrap_servers: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

impl Default for KafkaClientConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: kafka_bootstrap_servers(),
            properties: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct KafkaConfig {
    #[serde(flatten)]
    pub client: KafkaClientConfig,
    pub topic: String,
}

impl From<KafkaClientConfig> for ClientConfig {
    fn from(cfg: KafkaClientConfig) -> Self {
        let mut result = ClientConfig::new();
        result.set("bootstrap.servers", &cfg.bootstrap_servers);

        for (k, v) in cfg.properties {
            result.set(k.replace('_', "."), v);
        }

        result
    }
}

impl Deref for KafkaConfig {
    type Target = KafkaClientConfig;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

#[inline]
pub fn kafka_bootstrap_servers() -> String {
    "127.0.0.1:9092".into()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_custom() {
        std::env::set_var("KAFKA__PROPERTIES__A_B_C", "d.e.f");

        let env = config::Environment::with_prefix(&format!("{}_", "KAFKA"));

        let mut cfg = config::Config::new();
        cfg.merge(env.separator("__")).unwrap();
        let kafka: KafkaClientConfig = cfg.try_into().unwrap();

        assert_eq!(kafka.properties.get("a_b_c").cloned(), Some("d.e.f".into()));

        std::env::remove_var("KAFKA__PROPERTIES__A_B_C");
    }
}
