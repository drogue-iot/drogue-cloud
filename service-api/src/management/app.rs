use super::{Base64Standard, NonScopedMetadata};
use crate::{Dialect, Section, Translator};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Application {
    pub metadata: NonScopedMetadata,
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
pub struct ApplicationSpecTrustAnchors {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<ApplicationSpecTrustAnchorEntry>,
}

impl Dialect for ApplicationSpecTrustAnchors {
    fn key() -> &'static str {
        "trustAnchors"
    }

    fn section() -> Section {
        Section::Spec
    }
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

impl Dialect for ApplicationStatusTrustAnchors {
    fn key() -> &'static str {
        "trustAnchors"
    }

    fn section() -> Section {
        Section::Status
    }
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
