mod app;
mod session;
mod stream;

pub use app::App;

use drogue_cloud_service_api::kafka::KafkaClientConfig;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ServiceConfig {
    #[serde(default)]
    pub kafka: KafkaClientConfig,
    #[serde(default)]
    pub enable_username_password_auth: bool,
    #[serde(default)]
    pub disable_api_keys: bool,
}
