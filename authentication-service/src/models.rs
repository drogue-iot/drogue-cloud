use diesel::Queryable;
use serde_json::Value;

#[derive(Queryable, Clone)]
pub struct Credential {
    pub device_id: String,
    pub secret_type: i32,
    pub secret: Option<String>,
    pub properties: Option<Value>,
}
