mod commands;
mod source;

pub use commands::*;
pub use source::*;

use async_trait::async_trait;
use cloudevents::{AttributesReader, Event};
use drogue_cloud_service_common::Id;
use std::convert::{TryFrom, TryInto};
use thiserror::Error;

/// Represents command
#[derive(Clone, Debug)]
pub struct Command {
    pub device_id: Id,
    pub command: String,
    pub payload: Option<Vec<u8>>,
}

impl Command {
    /// Create a new scoped Command
    pub fn new<C: Into<String>>(device_id: Id, command: C, payload: Option<Vec<u8>>) -> Self {
        Self {
            device_id,
            command: command.into(),
            payload,
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum ParseCommandError {
    #[error("Missing attribute: {0}")]
    Missing(&'static str),
    #[error("Invalid payload")]
    Payload,
}

impl TryFrom<Event> for Command {
    type Error = ParseCommandError;

    fn try_from(mut event: Event) -> Result<Self, Self::Error> {
        let id = Id::from_event(&event).ok_or(ParseCommandError::Missing("ID"))?;

        let payload = if let (Some(data), ..) = event.take_data() {
            Some(data.try_into().map_err(|_| ParseCommandError::Payload)?)
        } else {
            None
        };

        let command = event
            .subject()
            .ok_or(ParseCommandError::Missing("Command"))?;

        Ok(Command::new(id, command, payload))
    }
}

/// Internally dispatch commands to the correct device.
#[async_trait]
pub trait CommandDispatcher {
    async fn send(&self, msg: Command) -> Result<(), String>;
}
