use super::{is_default, ScopedMetadata};
use crate::{Dialect, Section, Translator};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Device {
    pub metadata: ScopedMetadata,
    #[serde(default)]
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub spec: Map<String, Value>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub status: Map<String, Value>,
}

impl Translator for Device {
    fn spec(&self) -> &Map<String, Value> {
        &self.spec
    }

    fn status(&self) -> &Map<String, Value> {
        &self.status
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeviceSpecCore {
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub disabled: bool,
}

impl Dialect for DeviceSpecCore {
    fn key() -> &'static str {
        "core"
    }
    fn section() -> Section {
        Section::Spec
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeviceSpecCredentials {
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<Credential>,
}

impl Dialect for DeviceSpecCredentials {
    fn key() -> &'static str {
        "credentials"
    }
    fn section() -> Section {
        Section::Spec
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Credential {
    #[serde(rename = "user")]
    UsernamePassword {
        username: String,
        password: String,
        #[serde(default)]
        unique: bool,
    },
    #[serde(rename = "pass")]
    Password(String),
    #[serde(rename = "cert")]
    Certificate(String),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSpecGatewaySelector {
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub match_names: Vec<String>,
}

impl Dialect for DeviceSpecGatewaySelector {
    fn key() -> &'static str {
        "gatewaySelector"
    }
    fn section() -> Section {
        Section::Spec
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeviceSpecCommands {
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<Command>,
}

impl Dialect for DeviceSpecCommands {
    fn key() -> &'static str {
        "commands"
    }
    fn section() -> Section {
        Section::Spec
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Command {
    #[serde(rename = "external")]
    External(ExternalEndpoint),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalEndpoint {
    pub endpoint: String,
    pub r#type: Option<String>,
}
