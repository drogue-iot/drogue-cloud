//! Command router
//!
//! Routes commands to appropriate actors
//! Actors can subscribe/unsubscribe for commands by sending appropriate messages

use actix::prelude::*;
use actix_broker::BrokerSubscribe;
use cloudevents::event::ExtensionValue;
use cloudevents::Event;
use std::collections::HashMap;
use std::convert::TryFrom;

/// Represents command message passed to the actors
#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct CommandMessage {
    pub device_id: String,
    pub command: String,
}

/// Represents a message used to subscribe an actor for receiving the command
#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct CommandSubscribe(pub String, pub Device);

/// Represents a message used to unsubscribe an actor from receiving the command
#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct CommandUnsubscribe(pub String);

/// Recipient of commands
type Device = Recipient<CommandMessage>;

/// Routes commands to appropriate actors
#[derive(Default)]
pub struct CommandRouter {
    pub devices: HashMap<String, Device>,
}

impl CommandRouter {
    pub async fn send(event: Event) -> Result<(), String> {
        let device_id_ext = event.extension("deviceid");

        match device_id_ext {
            Some(ExtensionValue::String(device_id)) => {
                let command_msg = CommandMessage {
                    device_id: device_id.to_string(),
                    command: String::try_from(event.data().unwrap().clone()).unwrap(),
                };

                if let Err(e) = CommandRouter::from_registry().send(command_msg).await {
                    log::error!("Failed to route command: {}", e);
                    Err("Failed to route command".to_string())
                } else {
                    Ok(())
                }
            }
            _ => {
                log::error!("No device-id provided");
                Err("No device-id provided".to_string())
            }
        }
    }

    /// Subscribe actor to receive messages for a particular device
    fn subscribe(&mut self, id: String, device: Device) {
        log::debug!("Subscribe device for commands '{}'", id);

        self.devices.insert(id, device);
    }

    /// Unsubscribe actor from receiving messages for a particular device
    fn unsubscribe(&mut self, id: String) {
        log::info!("Unsubscribe device for commands '{}'", id);

        self.devices.remove(&id);
    }
}

impl Actor for CommandRouter {
    type Context = Context<Self>;

    /// Registers the router in the global registry when actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<CommandMessage>(ctx);
    }
}

impl Handler<CommandMessage> for CommandRouter {
    type Result = ();

    /// Routes received command messages
    fn handle(&mut self, msg: CommandMessage, _ctx: &mut Self::Context) -> Self::Result {
        match self.devices.get_mut(&msg.device_id) {
            Some(device) => {
                log::debug!("Sending command to the device '{}", msg.device_id);
                if let Err(e) = device.do_send(msg.to_owned()) {
                    log::error!("Failed to route command: {}", e);
                }
            }
            _ => log::debug!("No device '{}' present at this endpoint", &msg.device_id),
        }
    }
}

impl Handler<CommandSubscribe> for CommandRouter {
    type Result = ();

    /// Subscribes actors to receive commands for a particular device
    fn handle(&mut self, msg: CommandSubscribe, _ctx: &mut Self::Context) {
        let CommandSubscribe(id, device) = msg;

        self.subscribe(id, device);
    }
}

impl Handler<CommandUnsubscribe> for CommandRouter {
    type Result = ();

    /// Unsubscribes actors from receiving commands for a particular device
    fn handle(&mut self, msg: CommandUnsubscribe, _ctx: &mut Self::Context) {
        self.unsubscribe(msg.0);
    }
}

impl SystemService for CommandRouter {}
impl Supervised for CommandRouter {}
