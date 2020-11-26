use diesel::{Insertable, Queryable};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::credentials;

#[derive(Queryable, Clone, Serialize, Deserialize, Insertable)]
#[table_name = "credentials"]
pub struct Credential {
    pub device_id: String,
    pub secret_type: i32,
    pub secret: Option<String>,
    pub properties: Option<Value>,
}
