use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Deref};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct KafkaClientConfig {
    #[serde(default = "kafka_bootstrap_servers")]
    // although we have an alias specified, it currently doesn't work due to: https://github.com/serde-rs/serde/issues/1504
    #[serde(alias = "bootstrapServers")]
    pub bootstrap_servers: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

impl KafkaClientConfig {
    pub fn translate(mut self) -> Self {
        let mut result = HashMap::with_capacity(self.properties.len());
        for (k, v) in self.properties {
            result.insert(k.replace('_', "."), v);
        }
        self.properties = result;
        self
    }
}

impl Default for KafkaClientConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: kafka_bootstrap_servers(),
            properties: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct KafkaConfig {
    #[serde(flatten)]
    pub client: KafkaClientConfig,
    pub topic: String,
}

#[cfg(feature = "rdkafka")]
impl From<KafkaClientConfig> for rdkafka::ClientConfig {
    fn from(cfg: KafkaClientConfig) -> Self {
        let mut result = rdkafka::ClientConfig::new();
        result.set("bootstrap.servers", &cfg.bootstrap_servers);

        for (k, v) in cfg.properties {
            result.set(k.replace('_', "."), v);
        }

        result
    }
}

impl<'a> Deref for KafkaConfig {
    type Target = KafkaClientConfig;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

#[inline]
pub fn kafka_bootstrap_servers() -> String {
    "drogue-iot-kafka-bootstrap:9092".into()
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_custom() {
        let mut envs = HashMap::new();
        envs.insert(
            "KAFKA__BOOTSTRAP_SERVERS".to_string(),
            "localhost:1234".to_string(),
        );
        envs.insert("KAFKA__PROPERTIES__A_B_C".to_string(), "d.e.f".to_string());

        let env = config::Environment::with_prefix(&format!("{}", "KAFKA")).source(Some(envs));

        let cfg = config::Config::builder();
        let cfg = cfg.add_source(env.separator("__")).build().unwrap();
        let kafka: KafkaClientConfig = cfg.try_deserialize().unwrap();

        println!("{:?}", kafka);

        assert_eq!(kafka.bootstrap_servers, "localhost:1234");
        assert_eq!(kafka.properties.get("a_b_c").cloned(), Some("d.e.f".into()));
    }

    /// Test what we can also deserialize from JSON, in addition to the config crate.
    #[test]
    fn test_deserialize_json() {
        let kafka: KafkaClientConfig = serde_json::from_value(json!({
            "bootstrapServers": "localhost:9091"
        }))
        .unwrap();

        assert_eq!(kafka.bootstrap_servers, "localhost:9091")
    }
}
