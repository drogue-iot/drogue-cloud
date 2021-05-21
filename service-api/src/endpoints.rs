use crate::serde::is_default;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct HttpEndpoint {
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct MqttEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RegistryEndpoint {
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Endpoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt: Option<MqttEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_integration: Option<MqttEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sso: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<RegistryEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub demos: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub local_certs: bool,
}

impl Endpoints {
    pub fn publicize(&self) -> Endpoints {
        Endpoints {
            http: None,
            mqtt: None,
            mqtt_integration: None,
            sso: self.sso.clone(),
            issuer_url: self.issuer_url.clone(),
            redirect_url: None,
            registry: self.registry.clone(),
            command_url: None,
            demos: Vec::new(),
            local_certs: false,
        }
    }
}
