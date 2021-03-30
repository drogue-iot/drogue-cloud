use serde::Serialize;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
pub struct DrogueVersion {
    pub version: String,
}

impl DrogueVersion {
    pub fn get_version(&self) -> DrogueVersion {
        DrogueVersion {
            version: VERSION.to_string(),
        }
    }
}
