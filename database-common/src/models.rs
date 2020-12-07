use diesel::{Insertable, Queryable};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::credentials;

#[derive(Queryable, Clone, Serialize, Deserialize, Insertable, Debug)]
#[table_name = "credentials"]
pub struct Credential {
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub secret: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub properties: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Secret {
    pub hash: String,
    pub salt: String,
}

#[cfg(test)]
mod test {

    use super::*;
    use serde_json::json;

    #[test]
    fn test_de() -> Result<(), anyhow::Error> {
        let json = json!({
            "device_id": "12",
            "secret": "{}",
        });
        let credential: Credential = serde_json::from_value(json)?;

        assert_eq!(credential.device_id, "12");

        Ok(())
    }
}
