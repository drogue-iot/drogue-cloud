use drogue_cloud_service_api::endpoints::Endpoints;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointInformation {
    #[serde(flatten)]
    pub endpoints: Endpoints,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub demos: Vec<(String, String)>,
}

impl Deref for EndpointInformation {
    type Target = Endpoints;

    fn deref(&self) -> &Self::Target {
        &self.endpoints
    }
}
