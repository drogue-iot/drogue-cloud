use cloudevents::{event::ExtensionValue, Event, EventBuilderV10};
use drogue_cloud_service_api::{EXT_APPLICATION, EXT_DEVICE};
use serde::{Deserialize, Serialize};

/// A scoped device ID.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Id {
    pub app_id: String,
    pub device_id: String,
}

impl Id {
    /// Create a new scoped device ID.
    pub fn new<A: Into<String>, D: Into<String>>(app_id: A, device_id: D) -> Self {
        Self {
            app_id: app_id.into(),
            device_id: device_id.into(),
        }
    }

    /// Create a new ID from a cloud event.
    pub fn from_event(event: &Event) -> Option<Id> {
        let app_id_ext = event.extension(EXT_APPLICATION);
        let device_id_ext = event.extension(EXT_DEVICE);
        match (app_id_ext, device_id_ext) {
            (Some(ExtensionValue::String(app_id)), Some(ExtensionValue::String(device_id))) => {
                Some(Id::new(app_id, device_id))
            }
            _ => None,
        }
    }
}

pub trait IdInjector {
    fn inject(self, id: Id) -> Self;
}

impl IdInjector for EventBuilderV10 {
    fn inject(self, id: Id) -> Self {
        self.extension(EXT_APPLICATION, id.app_id)
            .extension(EXT_DEVICE, id.device_id)
    }
}
