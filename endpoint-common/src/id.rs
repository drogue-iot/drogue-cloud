use cloudevents::event::ExtensionValue;
use cloudevents::{Event, EventBuilderV10};

/// A scoped device ID.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Id {
    pub app_id: String,
    pub device_id: String,
}

impl Id {
    /// Create a new scoped device ID.
    pub fn new<A: ToString, D: ToString>(app_id: A, device_id: D) -> Self {
        Self {
            app_id: app_id.to_string(),
            device_id: device_id.to_string(),
        }
    }

    /// Create a new ID from a cloud event.
    pub fn from_event(event: &Event) -> Option<Id> {
        let app_id_ext = event.extension("application");
        let device_id_ext = event.extension("device");
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
        self.extension("application", id.app_id)
            .extension("device", id.device_id)
    }
}
