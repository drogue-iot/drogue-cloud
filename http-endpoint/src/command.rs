use actix::prelude::*;
use actix_broker::BrokerSubscribe;

use std::collections::HashMap;

use actix_web_actors::HttpContext;
use actix_web::web::Bytes;

use std::{time};

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct CommandMessage{
    pub device_id: String,
    pub command: String,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct CommandSubscribe(pub String, pub Device);

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct CommandUnsubscribe(pub String);


type Device = Recipient<CommandMessage>;

#[derive(Default)]
pub struct CommandRouter {
    pub devices: HashMap<String, Device>,
}

impl CommandRouter {

    fn subscribe(&mut self, id: String, device: Device) {

        log::debug!("Subscribing device for commands '{}'", id);

        self.devices.insert(id, device);
        
    }

    fn unsubscribe(&mut self, id: String) {
        
        log::info!("Unsubscribing device for commands '{}'", id);

        self.devices.remove(&id);

    }

}

impl Actor for CommandRouter {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<CommandMessage>(ctx);
    }
}

impl Handler<CommandMessage> for CommandRouter {
    type Result = ();

    fn handle(&mut self, msg: CommandMessage, _ctx: &mut Self::Context) -> Self::Result {

        match self.devices.get_mut(&msg.device_id) {
            Some(device) => {
                log::debug!("Sending command to the device '{}", msg.device_id);
                if let Err(e) = device.do_send(msg.to_owned()) {
                    log::error!("Failed to route command: {}", e);
                }
            }
            _ => {
                log::debug!("No device '{}' present at this endpoint", &msg.device_id)
            }
        }

    }
}


impl Handler<CommandSubscribe> for CommandRouter {
    type Result = ();

    fn handle(&mut self, msg: CommandSubscribe, _ctx: &mut Self::Context) {

        let CommandSubscribe(id, device) = msg;

        self.subscribe(id, device);


    }

}

impl Handler<CommandUnsubscribe> for CommandRouter {
    type Result = ();

    fn handle(&mut self, msg: CommandUnsubscribe, _ctx: &mut Self::Context) {

        self.unsubscribe(msg.0);


    }

}

impl SystemService for CommandRouter {}
impl Supervised for CommandRouter {}


pub struct CommandHandler{
    pub device_id: String,
    pub ttd: u64,
}

impl Actor for CommandHandler {
    type Context = HttpContext<Self>;

    fn started(&mut self, ctx: &mut HttpContext<Self>) {
        let sub = CommandSubscribe(self.device_id.to_owned(), ctx.address().recipient());
        CommandRouter::from_registry()
            .send(sub)
            .into_actor(self)
            .then(|result, _actor, _ctx| {
                match result {
                    Ok(_v) => {
                        log::debug!("Sent command subscribe request");
                    },
                    Err(e) => {
                        log::error!("Subscribe request failed: {}", e);
                    }
                }
                fut::ready(())
            })
            .wait(ctx);

        // Wait for ttd seconds for a command
        ctx.run_later(time::Duration::from_secs(self.ttd), |_slf, ctx| ctx.write_eof());
    }

    fn stopped(&mut self, ctx: &mut HttpContext<Self>) {
        CommandRouter::from_registry()
            .send(CommandUnsubscribe(self.device_id.to_owned()))
            .into_actor(self)
            .then(|result, _actor, _ctx| {
                match result {
                    Ok(_v) => {
                        log::debug!("Sent command unsubscribe request");
                    },
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

    fn handle(&mut self, msg: CommandMessage, ctx: &mut HttpContext<Self>) {
        ctx.write(Bytes::from(msg.command));
        ctx.write_eof()
    }
    
}
