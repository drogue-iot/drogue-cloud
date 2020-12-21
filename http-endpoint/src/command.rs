//! Command actors
//!
//! Contains actors that handles delivering commands between different components

use actix::prelude::*;
use actix_broker::BrokerSubscribe;

use std::collections::HashMap;

use actix_web::web::Bytes;
use actix_web_actors::HttpContext;

use std::time;

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

/// Recepient of commands
type Device = Recipient<CommandMessage>;

/// Routes commands to appropriate actors
/// Actors can subscribe/unsubscribe for commands by sending appropriate messages
#[derive(Default)]
pub struct CommandRouter {
    pub devices: HashMap<String, Device>,
}

impl CommandRouter {
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

/// Actor for receiving commands
pub struct CommandHandler {
    pub device_id: String,
    pub ttd: u64,
}

impl Actor for CommandHandler {
    type Context = HttpContext<Self>;

    /// Subscribes the actor with the command handler
    /// and waits for the command for `ttd` seconds
    fn started(&mut self, ctx: &mut HttpContext<Self>) {
        let sub = CommandSubscribe(self.device_id.to_owned(), ctx.address().recipient());
        CommandRouter::from_registry()
            .send(sub)
            .into_actor(self)
            .then(|result, _actor, _ctx| {
                match result {
                    Ok(_v) => {
                        log::debug!("Sent command subscribe request");
                    }
                    Err(e) => {
                        log::error!("Subscribe request failed: {}", e);
                    }
                }
                fut::ready(())
            })
            .wait(ctx);

        // Wait for ttd seconds for a command
        ctx.run_later(time::Duration::from_secs(self.ttd), |_slf, ctx| {
            ctx.write_eof()
        });
    }

    /// Unsubscribes the actor from receiving the commands
    fn stopped(&mut self, ctx: &mut HttpContext<Self>) {
        CommandRouter::from_registry()
            .send(CommandUnsubscribe(self.device_id.to_owned()))
            .into_actor(self)
            .then(|result, _actor, _ctx| {
                match result {
                    Ok(_v) => {
                        log::debug!("Sent command unsubscribe request");
                    }
                    Err(e) => {
                        log::error!("Unsubscribe request failed: {}", e);
                    }
                }
                fut::ready(())
            })
            .wait(ctx);
    }
}

impl Handler<CommandMessage> for CommandHandler {
    type Result = ();

    /// Handles q command message by writing it into the http context
    fn handle(&mut self, msg: CommandMessage, ctx: &mut HttpContext<Self>) {
        ctx.write(Bytes::from(msg.command));
        ctx.write_eof()
    }
}
