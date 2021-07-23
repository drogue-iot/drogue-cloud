use drogue_cloud_service_common::defaults;
use rdkafka::ClientConfig;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
pub struct KafkaClientConfig {
    #[serde(default = "defaults::kafka_bootstrap_servers")]
    pub bootstrap_servers: String,
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

impl From<KafkaClientConfig> for ClientConfig {
    fn from(cfg: KafkaClientConfig) -> Self {
        let mut result = ClientConfig::new();
        result.set("bootstrap.servers", &cfg.bootstrap_servers);

        for (k, v) in cfg.custom {
            result.set(k.replace('_', "."), v);
        }

        result
    }
}
