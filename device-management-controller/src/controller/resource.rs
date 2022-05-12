use crate::EventController;
use async_trait::async_trait;
use drogue_client::{error::ClientError, registry};
use drogue_cloud_operator_common::controller::{
    base::{Key, ResourceOperations},
    reconciler::ReconcileError,
};
use futures::try_join;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize)]
pub struct ApplicationAndDeviceKey {
    #[serde(rename = "a")]
    pub application: String,
    #[serde(rename = "d")]
    pub device: String,
    #[serde(rename = "u")]
    pub device_uid: String,
}

impl Key for ApplicationAndDeviceKey {
    fn to_string(&self) -> String {
        json!({"a": self.application, "d": self.device, "u": self.device_uid}).to_string()
    }

    fn from_string(s: String) -> Result<Self, &'static str> {
        serde_json::from_str(&s).map_err(|err| {
            log::warn!("Key deserialization error: {err}");
            "Key deserialization error"
        })
    }
}

#[derive(Clone, Debug)]
pub struct ApplicationAndDevice {
    pub key: ApplicationAndDeviceKey,

    pub application: registry::v1::Application,
    pub device: Option<registry::v1::Device>,
}

#[async_trait]
impl ResourceOperations<ApplicationAndDeviceKey, ApplicationAndDevice, ()> for EventController {
    async fn get(
        &self,
        key: &ApplicationAndDeviceKey,
    ) -> Result<Option<ApplicationAndDevice>, ClientError> {
        Ok(
            match try_join!(
                self.get_app(&key.application,),
                self.get_device(&key.application, &key.device,),
            )? {
                // app present, maybe device too
                (Some(application), device) => Some(ApplicationAndDevice {
                    key: key.clone(),
                    application,
                    device,
                }),
                _ => None,
            },
        )
    }

    async fn update_if(&self, _original: &(), mut _current: ()) -> Result<(), ReconcileError> {
        Ok(())
    }

    fn ref_output(_input: &ApplicationAndDevice) -> &() {
        &()
    }
}
