use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/*
 * This file might look like a some duplication of code. The main difference between
 * these two structs is that one has an "application" field, while the other doesn't.
 *
 * Rust doesn't make composing structs easy, so the simplest solution is to simply have two.
 *
 * Now both could be generated with a macro, to not repeat ourselves. However, macros need to
 * expand into a valid syntax tree element, which a list of fields is not. There is a macro pattern
 * called "muncher", which would allow use to create something like this. Then again, this isn't
 * really readable. Assuming that these structures are more often viewed than edited, it may be
 * simpler to keep them as they are.
 *
 * Should the need for processing both scoped and non-scoped metadata using the same method, we
 * would need to implement a `Metadata` and `MetadataMut` trait, which provides a common way to
 * access the metadata. For now, we don't require this.
 */

fn epoch() -> DateTime<Utc> {
    Utc.timestamp_millis(0)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct NonScopedMetadata {
    pub name: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub uid: String,

    /// The creation date of the resource.
    #[serde(default = "epoch")]
    pub creation_timestamp: DateTime<Utc>,
    /// A generation number, incrementing with each change.
    ///
    /// This increment between different version may be one, or greater than one.
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub resource_version: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletion_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub finalizers: Vec<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ScopedMetadata {
    pub application: String,
    pub name: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub uid: String,

    #[serde(default = "epoch")]
    pub creation_timestamp: DateTime<Utc>,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub resource_version: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletion_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub finalizers: Vec<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
}

impl Default for ScopedMetadata {
    fn default() -> Self {
        Self {
            application: Default::default(),
            name: Default::default(),
            uid: Default::default(),
            labels: Default::default(),
            annotations: Default::default(),
            creation_timestamp: chrono::Utc::now(),
            resource_version: Default::default(),
            generation: Default::default(),
            deletion_timestamp: Default::default(),
            finalizers: Default::default(),
        }
    }
}

impl Default for NonScopedMetadata {
    fn default() -> Self {
        Self {
            name: Default::default(),
            uid: Default::default(),
            labels: Default::default(),
            annotations: Default::default(),
            creation_timestamp: chrono::Utc::now(),
            resource_version: Default::default(),
            generation: Default::default(),
            deletion_timestamp: Default::default(),
            finalizers: Default::default(),
        }
    }
}
