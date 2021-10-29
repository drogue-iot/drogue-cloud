mod commands;
mod source;

pub use commands::*;
pub use source::*;

use async_trait::async_trait;
use cloudevents::{event::ExtensionValue, AttributesReader, Event};
use drogue_cloud_service_api::{EXT_APPLICATION, EXT_DEVICE, EXT_SENDER};
use std::convert::{TryFrom, TryInto};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CommandAddress {
    pub app_id: String,
    pub gateway_id: String,
    pub device_id: String,
}

impl CommandAddress {
    pub fn new<A, D, G>(app_id: A, gateway_id: G, device_id: D) -> Self
    where
        A: Into<String>,
        D: Into<String>,
        G: Into<String>,
    {
        Self {
            app_id: app_id.into(),
            device_id: device_id.into(),
            gateway_id: gateway_id.into(),
        }
    }

    /// Create a new CommandAddress from a cloud event.
    pub fn from_event(event: &Event) -> Option<CommandAddress> {
        let app_id_ext = event.extension(EXT_APPLICATION);
        let device_id_ext = event.extension(EXT_DEVICE);
        let sender_id = event.extension(EXT_SENDER).and_then(|v| match v {
            ExtensionValue::String(v) => Some(v),
            _ => None,
        });

        match (app_id_ext, device_id_ext) {
            (Some(ExtensionValue::String(app_id)), Some(ExtensionValue::String(device_id))) => {
                Some(CommandAddress::new(
                    app_id,
                    sender_id.map_or_else(|| device_id.as_str(), |s| s.as_str()),
                    device_id,
                ))
            }
            _ => None,
        }
    }
}

/// Represents command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    pub address: CommandAddress,
    pub command: String,
    pub payload: Option<Vec<u8>>,
}

impl Command {
    /// Create a new scoped Command
    pub fn new<C: Into<String>>(
        address: CommandAddress,
        command: C,
        payload: Option<Vec<u8>>,
    ) -> Self {
        Self {
            address,
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
        let address =
            CommandAddress::from_event(&event).ok_or(ParseCommandError::Missing("Address"))?;

        let payload = if let (_, _, Some(data)) = event.take_data() {
            Some(data.try_into().map_err(|_| ParseCommandError::Payload)?)
        } else {
            None
        };

        let command = event
            .subject()
            .ok_or(ParseCommandError::Missing("Command"))?;

        Ok(Command::new(address, command, payload))
    }
}

/// Internally dispatch commands to the correct device.
#[async_trait]
pub trait CommandDispatcher {
    async fn send(&self, msg: Command);
}
