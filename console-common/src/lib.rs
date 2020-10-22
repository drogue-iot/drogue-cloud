use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpEndpoint {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MqttEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Endpoints {
    pub http: Option<HttpEndpoint>,
    pub mqtt: Option<MqttEndpoint>,
}
