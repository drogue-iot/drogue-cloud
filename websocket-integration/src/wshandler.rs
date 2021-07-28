use crate::messages::{Disconnect, Subscribe, WsEvent};
use crate::service::Service;
use actix::prelude::*;
use actix::{
    fut, Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, ContextFutureSpawner, Handler,
    Running, WrapFuture,
};
use actix_web_actors::ws;
use actix_web_actors::ws::Message::Text;
use std::time::{Duration, Instant};
use uuid::Uuid;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

// This is the actor handling one websocket connection.
pub struct WsHandler {
    // the topic to listen to
    application: String,
    // to exit the actor if the client was disconnected
    heartbeat: Instant,
    service_addr: Addr<Service>,
    id: Uuid,
}

impl WsHandler {
    pub fn new(app: String, service_addr: Addr<Service>) -> WsHandler {
        WsHandler {
            application: app,
            heartbeat: Instant::now(),
            service_addr,
            id: Uuid::new_v4(),
        }
    }
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.heartbeat) > CLIENT_TIMEOUT {
                log::warn!("Disconnecting failed heartbeat");
                ctx.stop();
                return;
            }

            ctx.ping(b"PING");
        });
    }
}

// Implement actix Actor trait
impl Actor for WsHandler {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("Starting WS handler");
        self.heartbeat(ctx);

        // Address of self, the WSHandler actor
        let addr = ctx.address();
        // Send a message to ask service to subscribe to Kafka stream.
        self.service_addr
            .send(Subscribe {
                addr: addr.recipient(),
                application: self.application.clone(),
                id: self.id,
            })
            // We need to access the context when handling the future so we wrap it into an ActorFuture
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(_) => {
                        log::info!("Subscribe request for {} successful", act.application);
                    }
                    _ => {
                        log::error!("Subscribe request for {} failed", act.application);
                        ctx.stop()
                    }
                };
                fut::ready(())
            })
            .wait(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        self.service_addr.do_send(Disconnect { id: self.id });
        Running::Stop
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        log::debug!("Terminated WebSocket actor {}", self.id);
    }
}

// Handle incoming messages from the Websocket Client
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsHandler {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.heartbeat = Instant::now();
            }
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(reason)) => {
                log::debug!("Client disconnected");
                ctx.close(reason);
                ctx.stop();
            }
            Ok(ws::Message::Continuation(_)) => {
                ctx.stop();
            }
            Ok(ws::Message::Nop) => (),
            Ok(Text(s)) => log::debug!("Recevied text from client in websocket :\n{}", s),
            Err(e) => {
                log::error!("WebSocket Protocol Error: {}", e);
                ctx.stop()
            }
        }
    }
}

// Handle incoming messages from the Service
// Forward them to websocket Client
impl Handler<WsEvent> for WsHandler {
    type Result = ();

    fn handle(&mut self, msg: WsEvent, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}
