use crate::serde::is_default;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CoapEndpoint {
    pub url: String,
}

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
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coap: Option<CoapEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt: Option<MqttEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_ws: Option<HttpEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_ws_browser: Option<HttpEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_integration: Option<MqttEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_integration_ws: Option<HttpEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_integration_ws_browser: Option<HttpEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket_integration: Option<HttpEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sso: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<RegistryEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_url: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub local_certs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kafka_bootstrap_servers: Option<String>,
}

impl Endpoints {
    pub fn publicize(&self) -> Endpoints {
        Endpoints {
            coap: None,
            http: None,
            mqtt: None,
            mqtt_ws: None,
            mqtt_ws_browser: None,
            mqtt_integration: None,
            mqtt_integration_ws: None,
            mqtt_integration_ws_browser: None,
            websocket_integration: None,
            sso: self.sso.clone(),
            api: self.api.clone(),
            console: self.console.clone(),
            issuer_url: self.issuer_url.clone(),
            redirect_url: None,
            registry: self.registry.clone(),
            command_url: None,
            kafka_bootstrap_servers: None,
            local_certs: false,
        }
    }
}
