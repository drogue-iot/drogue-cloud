//! Http context command handlers
//!
//! Contains actors that handles commands for HTTP endpoint

use actix::prelude::*;

use drogue_cloud_endpoint_common::command_router::{
    CommandMessage, CommandRouter, CommandSubscribe, CommandUnsubscribe,
};

use actix_web::web::Bytes;
use actix_web_actors::HttpContext;

use std::time;

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
