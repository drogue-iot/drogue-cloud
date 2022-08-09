use crate::{
    messages::{Disconnect, Protocol, StreamError, Subscribe, WsEvent},
    service::Service,
    CONNECTIONS_COUNTER,
};
use actix::{
    fut, prelude::*, Actor, ActorContext, ActorFutureExt, Addr, AsyncContext, ContextFutureSpawner,
    Handler, Running, WrapFuture,
};
use actix_web_actors::ws::{self, CloseReason};
use chrono::{DateTime, TimeZone, Utc};
use drogue_client::{
    integration::ws::v1::client,
    user::{self, v1::authz},
};
use drogue_cloud_service_api::{auth::user::UserInformation, webapp::http::ws::CloseCode};
use drogue_cloud_service_common::auth::openid::{self, CustomClaims};
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

#[derive(Clone)]
struct AuthContext {
    application: String,
    /// authenticator for refreshing the token
    authenticator: openid::Authenticator,
    /// user authorizer
    user_auth: user::v1::Client,
}

enum AuthOutcome {
    Allow(DateTime<Utc>),
    Deny,
}

impl AuthContext {
    /// Validate the openid (only) access token.
    ///
    /// We do not support credential types!
    async fn validate_token(&self, token: String) -> Result<AuthOutcome, anyhow::Error> {
        let token = self.authenticator.validate_token(token).await?;
        let user = UserInformation::Authenticated(token.clone().into());

        match self
            .user_auth
            .authorize(authz::AuthorizationRequest {
                application: self.application.clone(),
                permission: authz::Permission::Read,
                user_id: user.user_id().map(ToString::to_string),
                roles: user.roles().clone(),
            })
            .await?
        {
            authz::AuthorizationResponse {
                outcome: authz::Outcome::Allow,
            } => Ok(AuthOutcome::Allow(
                Utc.timestamp(token.standard_claims().exp, 0),
            )),
            authz::AuthorizationResponse {
                outcome: authz::Outcome::Deny,
            } => Ok(AuthOutcome::Deny),
        }
    }
}

/// This is the actor handling one websocket connection.
pub struct WsHandler {
    /// the topic to listen to
    application: String,
    /// the optional consumer group
    group_id: Option<String>,
    /// to exit the actor if the client was disconnected
    heartbeat: Instant,
    service_addr: Addr<Service>,
    id: Uuid,
    /// When the JWT expires, represented as the number of seconds from epoch
    /// It's optional, as some clients will use an access token, which are valid indefinitely
    auth_expiration: Option<DateTime<Utc>>,
    auth_context: Option<AuthContext>,
}

impl WsHandler {
    pub fn new(
        application: String,
        group_id: Option<String>,
        service_addr: Addr<Service>,
        auth_expiration: Option<DateTime<Utc>>,
        authenticator: Option<openid::Authenticator>,
        user_auth: Option<user::v1::Client>,
    ) -> WsHandler {
        CONNECTIONS_COUNTER.inc();

        let auth_context = match (authenticator, user_auth) {
            (Some(authenticator), Some(user_auth)) => Some(AuthContext {
                application: application.clone(),
                authenticator,
                user_auth,
            }),
            _ => None,
        };

        WsHandler {
            application,
            group_id,
            heartbeat: Instant::now(),
            service_addr,
            id: Uuid::new_v4(),
            auth_expiration,
            auth_context,
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
        ctx.run_interval(AUTH_CHECK_INTERVAL, move |act, ctx| {
            if let Some(expiration) = act.auth_expiration {
                if Utc::now() > expiration {
                    log::info!("Disconnecting client: JWT token expired");
                    ctx.close(Some(CloseReason {
                        code: CloseCode::Policy,
                        description: Some("JWT token expired".to_string()),
                    }));
                    ctx.stop();
                }
            }
        });
    }

    /// Handle the parse result of a client protocol message.
    fn handle_protocol_message(
        ctx: &mut ws::WebsocketContext<Self>,
        result: Result<client::Message, serde_json::Error>,
    ) {
        match result {
            Ok(msg) => ctx.address().do_send(Protocol(msg)),
            Err(err) => {
                ctx.close(Some(CloseReason {
                    code: CloseCode::Protocol,
                    description: Some(err.to_string()),
                }));
                ctx.stop();
            }
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
            Ok(ws::Message::Binary(data)) => {
                INCOMING_MESSAGE.with_label_values(&["binary"]).inc();
                Self::handle_protocol_message(
                    ctx,
                    serde_json::from_slice::<client::Message>(&data),
                );
            }
            Ok(ws::Message::Text(data)) => {
                INCOMING_MESSAGE.with_label_values(&["text"]).inc();
                Self::handle_protocol_message(ctx, serde_json::from_str::<client::Message>(&data));
            }
            Ok(ws::Message::Close(reason)) => {
                INCOMING_MESSAGE.with_label_values(&["close"]).inc();
                log::debug!("Client disconnected - reason: {:?}", reason);
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
        ctx.close(Some(CloseReason {
            code: CloseCode::Error,
            description: Some(msg.error.to_string()),
        }));
        ctx.stop()
    }
}

impl Handler<Protocol> for WsHandler {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: Protocol, _ctx: &mut Self::Context) -> Self::Result {
        match msg.0 {
            client::Message::RefreshAccessToken(token) => {
                let auth_context = self.auth_context.clone();

                Box::pin(
                    async {
                        if let Some(auth_context) = auth_context {
                            auth_context.validate_token(token).await
                        } else {
                            Err(anyhow::anyhow!("Token authentication is not enabled"))
                        }
                    }
                    .into_actor(self)
                    .map(|result, act, ctx| {
                        match result {
                            Ok(outcome) => {
                                match outcome {
                                    AuthOutcome::Allow(auth_expiration) => {
                                        // set new token
                                        log::info!(
                                            "Updating token expiration: {:?} -> {:?}",
                                            act.auth_expiration,
                                            auth_expiration
                                        );
                                        act.auth_expiration = Some(auth_expiration);
                                    }
                                    AuthOutcome::Deny => {
                                        ctx.close(Some(CloseReason {
                                            code: CloseCode::Policy,
                                            description: Some(
                                                "Failed to refresh token".to_string(),
                                            ),
                                        }));
                                        ctx.stop();
                                    }
                                }
                            }
                            Err(err) => {
                                ctx.close(Some(CloseReason {
                                    code: CloseCode::Error,
                                    description: Some(format!("Failed to refresh token: {err}")),
                                }));
                                ctx.stop();
                            }
                        }
                    }),
                )
            }
        }
    }
}
