use crate::Translator;
use base64_serde::base64_serde_type;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

base64_serde_type!(Base64Standard, base64::STANDARD);

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
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
pub struct Application {
    pub metadata: ApplicationMetadata,
    #[serde(default)]
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub spec: Map<String, Value>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub status: Map<String, Value>,
}

impl Translator for Application {
    fn spec(&self) -> &Map<String, Value> {
        &self.spec
    }

    fn status(&self) -> &Map<String, Value> {
        &self.status
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApplicationMetadata {
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApplicationSpecTrustAnchors {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<ApplicationSpecTrustAnchorEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApplicationSpecTrustAnchorEntry {
    #[serde(with = "Base64Standard")]
    pub certificate: Vec<u8>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ApplicationStatusTrustAnchors {
    pub anchors: Vec<ApplicationStatusTrustAnchorEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationStatusTrustAnchorEntry {
    #[serde(rename_all = "camelCase")]
    Valid {
        subject: String,
        #[serde(with = "Base64Standard")]
        certificate: Vec<u8>,
        not_before: DateTime<Utc>,
        not_after: DateTime<Utc>,
    },
    Invalid {
        error: String,
        message: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Device {
    pub metadata: DeviceMetadata,
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
pub struct DeviceMetadata {
    pub application: String,
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeviceSpecCore {
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub disabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DeviceSpecCredentials {
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<Credential>,
}
