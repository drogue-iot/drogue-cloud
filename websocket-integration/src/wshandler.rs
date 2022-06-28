use crate::{
    messages::{Disconnect, StreamError, Subscribe, WsEvent},
    service::Service,
    CONNECTIONS_COUNTER,
};
use actix::{
    fut, prelude::*, Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, ContextFutureSpawner,
    Handler, Running, WrapFuture,
};
use actix_web_actors::ws;
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use prometheus::{register_int_counter_vec, IntCounterVec};
use std::time::{Duration, Instant};
use uuid::Uuid;

const AUTH_CHECK_INTERVAL: Duration = Duration::from_secs(90);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

lazy_static! {
    pub static ref INCOMING_MESSAGE: IntCounterVec = register_int_counter_vec!(
        "drogue_ws_integration_incoming_message",
        "Incoming WS integration message",
        &["type"]
    )
    .unwrap();
}

// This is the actor handling one websocket connection.
pub struct WsHandler {
    // the topic to listen to
    application: String,
    // the optional consumer group
    group_id: Option<String>,
    // to exit the actor if the client was disconnected
    heartbeat: Instant,
    service_addr: Addr<Service>,
    id: Uuid,
    // When the JWT expires, represented as the number of seconds from epoch
    // It's optional, as some clients will use an access token, which are valid indefinitely
    auth_expiration: Option<DateTime<Utc>>,
}

impl WsHandler {
    pub fn new(
        app: String,
        group_id: Option<String>,
        service_addr: Addr<Service>,
        auth_expiration: Option<DateTime<Utc>>,
    ) -> WsHandler {
        CONNECTIONS_COUNTER.inc();
        WsHandler {
            application: app,
            group_id,
            heartbeat: Instant::now(),
            service_addr,
            id: Uuid::new_v4(),
            auth_expiration,
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

    fn check_token_expiration(&self, ctx: &mut ws::WebsocketContext<Self>) {
        if let Some(expiration) = self.auth_expiration {
            ctx.run_interval(AUTH_CHECK_INTERVAL, move |_act, ctx| {
                if Utc::now() > expiration {
                    log::warn!("Disconnecting client : JWT token expired");
                    ctx.stop();
                }
            });
        }
    }
}

impl Drop for WsHandler {
    fn drop(&mut self) {
        CONNECTIONS_COUNTER.dec();
    }
}

// Implement actix Actor trait
impl Actor for WsHandler {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("Starting WS handler");
        self.heartbeat(ctx);
        self.check_token_expiration(ctx);

        // Address of self, the WSHandler actor
        let addr: Recipient<WsEvent> = ctx.address().recipient();
        let err_addr: Recipient<StreamError> = ctx.address().recipient();
        // Send a message to ask service to subscribe to Kafka stream.
        self.service_addr
            .send(Subscribe {
                addr,
                err_addr,
                application: self.application.clone(),
                consumer_group: self.group_id.clone(),
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
                INCOMING_MESSAGE.with_label_values(&["ping"]).inc();
                self.heartbeat = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                INCOMING_MESSAGE.with_label_values(&["pong"]).inc();
                self.heartbeat = Instant::now();
            }
            Ok(ws::Message::Binary(bin)) => {
                INCOMING_MESSAGE.with_label_values(&["binary"]).inc();
                ctx.binary(bin)
            }
            Ok(ws::Message::Text(s)) => {
                INCOMING_MESSAGE.with_label_values(&["text"]).inc();
                log::debug!("Received text from client {}:\n{}", self.id, s)
            }
            Ok(ws::Message::Close(reason)) => {
                INCOMING_MESSAGE.with_label_values(&["close"]).inc();
                log::debug!("Client disconnected");
                ctx.close(reason);
                ctx.stop();
            }
            Ok(ws::Message::Continuation(_)) => {
                INCOMING_MESSAGE.with_label_values(&["continuation"]).inc();
                ctx.stop();
            }
            Ok(ws::Message::Nop) => {
                INCOMING_MESSAGE.with_label_values(&["nop"]).inc();
            }
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

// Handle errors from the Service
impl Handler<StreamError> for WsHandler {
    type Result = ();

    fn handle(&mut self, msg: StreamError, ctx: &mut Self::Context) {
        log::error!(
            "Service encountered an error with the stream: {}",
            msg.error
        );
        ctx.text(format!("{:?}", msg.error));
        ctx.stop()
    }
}
