pub mod app;

use drogue_cloud_service_api::kafka::KafkaClientConfig;
use serde::Deserialize;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerConfig {
    pub ditto_devops: DittoDevops,
    pub kafka: KafkaClientConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DittoDevops {
    pub url: Url,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}
