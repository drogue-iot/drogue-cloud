//! Http context command handlers
//!
//! Contains actors that handles commands for HTTP endpoint

use actix::prelude::*;
use actix_web::{http, web::Bytes, HttpResponse};
use actix_web_actors::HttpContext;
use drogue_cloud_endpoint_common::command_router::{
    CommandMessage, CommandRouter, CommandSubscribe, CommandUnsubscribe,
};
use drogue_cloud_endpoint_common::error::HttpEndpointError;
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

/// Waits for a command for a `ttd_param` seconds by creating a command handler actor
pub async fn command_wait<T: Into<String>, D: Into<String>>(
    _tenant_id: T,
    device_id: D,
    ttd_param: Option<u64>,
    status: http::StatusCode,
) -> Result<HttpResponse, HttpEndpointError> {
    match ttd_param {
        Some(ttd) => {
            let handler = CommandHandler {
                device_id: device_id.into(),
                ttd,
            };
            let context = HttpContext::create(handler);
            Ok(HttpResponse::build(status).streaming(context))
        }
        _ => Ok(HttpResponse::build(status).finish()),
    }
}
