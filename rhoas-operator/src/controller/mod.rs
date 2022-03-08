pub mod app;

use drogue_cloud_service_common::openid::TokenConfig;
use serde::Deserialize;

const DEFAULT_BASE_PATH: &str = "";

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerConfig {
    /// The SASL mechanism to inject into the user status section
    #[serde(default = "default::sasl_mechanism")]
    pub sasl_mechanism: String,

    /// The API configuration
    pub api: ApiConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApiConfig {
    #[serde(default)]
    pub oauth2: Option<TokenConfig>,

    #[serde(default)]
    pub base_path: Option<String>,

    #[serde(default)]
    pub mgmt_base_path: Option<String>,
    #[serde(default)]
    pub instance_base_path: Option<String>,
}

impl ApiConfig {
    pub fn mgmt_base_path(&self) -> String {
        self.mgmt_base_path
            .clone()
            .or_else(|| self.base_path.clone())
            .unwrap_or_else(|| DEFAULT_BASE_PATH.into())
    }

    pub fn instance_base_path(&self) -> String {
        self.instance_base_path
            .clone()
            .or_else(|| self.base_path.clone())
            .unwrap_or_else(|| DEFAULT_BASE_PATH.into())
    }
}

mod default {
    pub fn sasl_mechanism() -> String {
        "SCRAM-SHA-512".into()
    }
}
