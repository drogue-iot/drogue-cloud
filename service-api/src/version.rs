use serde::{Deserialize, Serialize};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrogueVersion {
    pub version: String,
}

impl DrogueVersion {
    pub fn new() -> DrogueVersion {
        DrogueVersion {
            version: VERSION.to_string(),
        }
    }
}

impl Default for DrogueVersion {
    fn default() -> Self {
        Self::new()
    }
}

impl ToString for DrogueVersion {
    fn to_string(&self) -> String {
        self.version.clone()
    }
}
