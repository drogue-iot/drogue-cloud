use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Device {
    pub tenant_id: String,
    pub id: String,
    pub data: DeviceData,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct DeviceData {
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<Credential>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Value::is_null")]
    pub properties: Value,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,

    pub data: TenantData,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TenantData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub disabled: bool,
}
