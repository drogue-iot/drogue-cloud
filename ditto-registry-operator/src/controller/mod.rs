pub mod app;
pub mod device;

use crate::ditto::data::EntityId;
use drogue_client::registry::v1::Application;
use drogue_cloud_service_api::kafka::KafkaClientConfig;
use drogue_cloud_service_common::openid::TokenConfig;
use serde::Deserialize;
use url::Url;

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerConfig {
    pub ditto_devops: DittoDevops,
    pub ditto_admin: TokenConfig,
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

pub fn policy_id(app: &Application) -> EntityId {
    EntityId(app.metadata.name.clone(), "default".to_string())
}
