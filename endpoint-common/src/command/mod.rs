mod commands;
mod source;

pub use commands::*;
pub use source::*;

use async_trait::async_trait;
use cloudevents::{AttributesReader, Event};
use drogue_cloud_service_common::Id;
use std::convert::TryFrom;

/// Represents command
#[derive(Clone, Debug)]
pub struct Command {
    pub device_id: Id,
    pub command: String,
    pub payload: Option<String>,
}

impl Command {
    /// Create a new scoped Command
    pub fn new<C: Into<String>>(device_id: Id, command: C, payload: Option<String>) -> Self {
        Self {
            device_id,
            command: command.into(),
            payload,
        }
    }
}

impl TryFrom<Event> for Command {
    type Error = ();

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match Id::from_event(&event) {
            Some(device_id) => Ok(Command::new(
                device_id,
                event.subject().unwrap().to_string(),
                String::try_from(event.data().unwrap().clone()).ok(),
            )),
            _ => Err(()),
        }
    }
}

/// Internally dispatch commands to the correct device.
#[async_trait]
pub trait CommandDispatcher {
    async fn send(&self, msg: Command) -> Result<(), String>;
}
