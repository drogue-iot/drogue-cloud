use crate::{error::ServerError, mqtt::*};
use cloudevents::Data;
use drogue_client::{registry, Context};
use drogue_cloud_endpoint_common::downstream::DownstreamSender;
use drogue_cloud_integration_common::{
    self,
    commands::CommandOptions,
    stream::{EventStream, EventStreamConfig},
};
use drogue_cloud_service_api::auth::user::{
    authn::{AuthenticationRequest, Outcome},
    authz::{self, AuthorizationRequest, Permission},
    UserInformation,
};
use drogue_cloud_service_common::{
    client::UserAuthClient,
    defaults,
    openid::{Authenticator, AuthenticatorError},
};
use futures::StreamExt;
use ntex::util::{ByteString, Bytes};
use ntex_mqtt::{types::QoS, v5};
use serde::Deserialize;
use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;

#[derive(Clone, Debug, Deserialize)]
pub struct ServiceConfig {
    #[serde(default = "defaults::kafka_bootstrap_servers")]
    pub kafka_bootstrap_servers: String,
    pub kafka_topic: String,
    #[serde(default)]
    pub enable_username_password_auth: bool,
    #[serde(default)]
    pub disable_api_keys: bool,
}

#[derive(Clone, Debug)]
pub struct App {
    pub authenticator: Option<Authenticator>,
    pub user_auth: Option<Arc<UserAuthClient>>,
    pub config: ServiceConfig,
    pub sender: DownstreamSender,
    pub registry: registry::v1::Client,
}

pub struct Session {
    pub config: ServiceConfig,
    pub user_auth: Option<Arc<UserAuthClient>>,

    pub sink: Sink,
    pub client_id: String,
    pub user: UserInformation,

    streams: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,

    pub sender: DownstreamSender,
    pub registry: registry::v1::Client,

    pub token: Option<String>,
}

struct Stream {
    pub topic: ByteString,
    pub id: Option<NonZeroU32>,
    pub event_stream: EventStream,
    pub content_mode: ContentMode,
}

impl Drop for Stream {
    fn drop(&mut self) {
        log::info!("Dropped stream - topic: {}", self.topic);
    }
}

#[derive(Clone, Copy, Debug)]
enum ContentMode {
    Binary,
    Structured,
}

impl App {
    /// Authenticate a connection from a connect packet
    async fn authenticate<Io>(
        &self,
        connect: &Connect<'_, Io>,
        auth: &Authenticator,
    ) -> Result<UserInformation, anyhow::Error> {
        let user = match (connect.credentials(), &self.user_auth) {
            ((Some(username), Some(password)), Some(user_auth)) => {
                log::debug!("Authenticate with username and password");
                // we have a username and password, and are allowed to test this against SSO
                let username = username.to_string();
                let password = String::from_utf8(password.to_vec())?;

                match user_auth
                    .authenticate_api_key(
                        AuthenticationRequest {
                            user_id: username,
                            api_key: password,
                        },
                        Context::default(),
                    )
                    .await?
                    .outcome
                {
                    Outcome::Known(details) => UserInformation::Authenticated(details),
                    Outcome::Unknown => {
                        log::debug!("Unknown API key");
                        return Err(AuthenticatorError::Failed.into());
                    }
                }
            }
            ((Some(username), None), _) => {
                log::debug!("Authenticate with token (username only)");
                // username but no username is treated as a token
                let token = auth.validate_token(&username).await?.into();
                UserInformation::Authenticated(token)
            }
            ((None, Some(password)), _) => {
                log::debug!("Authenticate with token (password only)");
                // password but no username is treated as a token
                let password = String::from_utf8(password.to_vec())?;
                let token = auth.validate_token(&password).await?.into();
                UserInformation::Authenticated(token)
            }
            ((None, None), _) => {
                // anonymous authentication, but using user auth
                log::debug!("Anonymous auth");
                UserInformation::Anonymous
            }
            _ => {
                log::debug!("Unknown authentication method");
                anyhow::bail!("Unknown authentication scheme");
            }
        };

        Ok(user)
    }

    pub async fn connect<Io>(&self, connect: Connect<'_, Io>) -> Result<Session, ServerError> {
        log::debug!("Processing connect request");

        if !connect.clean_session() {
            return Err(ServerError::UnsupportedOperation);
        }

        let user = if let Some(auth) = &self.authenticator {
            // authenticate
            self.authenticate(&connect, auth)
                .await
                .map_err(|_| ServerError::AuthenticationFailed)?
        } else {
            // we are running without authentication
            UserInformation::Anonymous
        };

        let client_id = connect.client_id().to_string();

        let token = match connect.credentials().1 {
            Some(token) => match String::from_utf8(token.to_vec()) {
                Ok(token_string) => Some(token_string),
                Err(_) => None,
            },
            None => None,
        };

        Ok(Session::new(
            self.config.clone(),
            self.user_auth.clone(),
            connect.sink(),
            user,
            client_id,
            self.sender.clone(),
            self.registry.clone(),
            token,
        ))
    }
}

impl Session {
    pub fn new(
        config: ServiceConfig,
        user_auth: Option<Arc<UserAuthClient>>,
        sink: Sink,
        user: UserInformation,
        client_id: String,
        sender: DownstreamSender,
        registry: registry::v1::Client,
        token: Option<String>,
    ) -> Self {
        Session {
            config,
            user_auth,
            user,
            sink,
            client_id,
            streams: Arc::new(Mutex::new(HashMap::new())),
            sender,
            registry,
            token,
        }
    }

    async fn authorize(
        &self,
        application: String,
        user_auth: &Arc<UserAuthClient>,
    ) -> Result<(), v5::codec::SubscribeAckReason> {
        let response = user_auth
            .authorize(
                AuthorizationRequest {
                    application,
                    permission: Permission::Read,
                    user_id: self.user.user_id().map(ToString::to_string),
                    roles: self.user.roles().clone(),
                },
                Default::default(),
            )
            .await
            .map_err(|_| v5::codec::SubscribeAckReason::NotAuthorized)?;

        match response.outcome {
            authz::Outcome::Allow => Ok(()),
            authz::Outcome::Deny => Err(v5::codec::SubscribeAckReason::NotAuthorized),
        }
    }

    async fn subscribe_to(
        &self,
        id: Option<NonZeroU32>,
        original_topic: String,
        content_mode: ContentMode,
    ) -> Result<QoS, v5::codec::SubscribeAckReason> {
        // split topic into path segments
        let topic = original_topic.split('/').collect::<Vec<_>>();

        // extract the shared named, which we use as kafka consumer group id
        let (group_id, topic) = match topic.as_slice() {
            ["$shared", group_id, topic @ ..] => (Some(group_id), topic),
            other => (None, other),
        };

        // check for wildcard subscriptions
        if topic.iter().any(|seg| *seg == "+" || *seg == "#") {
            return Err(v5::codec::SubscribeAckReason::WildcardSubscriptionsNotSupported);
        }

        let app = match topic {
            [] => Err(v5::codec::SubscribeAckReason::NotAuthorized),
            ["a", application] => Ok(application),
            ["app", application] => Ok(application),
            ["application", application] => Ok(application),
            _ => Err(v5::codec::SubscribeAckReason::TopicFilterInvalid),
        }?;

        // scope the group id, as we currently only have a single kafka topic

        let group_id = group_id.map(|g| format!("{}:{}", app, g));

        // log the request

        log::debug!(
            "Request to subscribe to app: {} (group: {:?})",
            app,
            group_id
        );

        // authorize topic for user

        match &self.user_auth {
            Some(user_auth) => {
                // authenticated user
                self.authorize(app.to_string(), user_auth).await?;
            }
            None => {
                // authorization disabled ... nothing to do
            }
        }

        // create stream

        let stream = EventStream::new(EventStreamConfig {
            bootstrap_servers: self.config.kafka_bootstrap_servers.clone(),
            topic: self.config.kafka_topic.clone(),
            app: app.to_string(),
            consumer_group: group_id,
        })
        .map_err(|err| {
            log::info!("Failed to subscribe to Kafka topic: {}", err);
            v5::codec::SubscribeAckReason::UnspecifiedError
        })?;

        // we started the stream, now hold on to it ...

        let stream = Stream {
            topic: original_topic.into(),
            id,
            event_stream: stream,
            content_mode,
        };

        self.attach_app(stream);

        // done

        Ok(QoS::AtMostOnce)
    }

    pub async fn subscribe(&self, subscribe: Subscribe<'_>) -> Result<(), ServerError> {
        let id = subscribe.id();
        log::debug!("Subscription ID: {:?}", id);

        let user_properties = subscribe.user_properties();

        // evaluate the content mode

        let content_mode = {
            let value = user_properties.and_then(|props| {
                props
                    .iter()
                    .find(|(k, _)| k == "content-mode")
                    .map(|(_, v)| v.to_string())
            });
            match value.as_deref() {
                None | Some("structured") => ContentMode::Structured,
                Some("binary") => ContentMode::Binary,
                Some(other) => {
                    log::info!("Unknown content mode: {}", other);
                    return Err(ServerError::UnsupportedOperation);
                }
            }
        };

        log::debug!("Content mode: {:?}", content_mode);

        for mut sub in subscribe {
            let res = self
                .subscribe_to(id, sub.topic().to_string(), content_mode)
                .await;
            log::debug!("Subscribing to: {:?} -> {:?}", sub.topic(), res);
            match res {
                Ok(qos) => sub.confirm(qos),
                Err(reason) => sub.fail(reason),
            }
        }

        Ok(())
    }

    pub async fn unsubscribe(&self, unsubscribe: Unsubscribe<'_>) -> Result<(), ServerError> {
        for unsub in unsubscribe {
            log::debug!("Unsubscribe: {:?}", unsub.topic());
            if !self.detach(unsub.topic().as_ref()) {
                // failed to unsubscribe
                match unsub {
                    Unsubscription::V3(_) => {
                        // for v3 we do nothing, as no subscription existed
                    }
                    Unsubscription::V5(mut unsub) => {
                        unsub.fail(v5::codec::UnsubscribeAckReason::NoSubscriptionExisted)
                    }
                }
            } else {
                match unsub {
                    Unsubscription::V3(_) => {}
                    Unsubscription::V5(mut unsub) => unsub.success(),
                }
            }
        }

        Ok(())
    }

    pub async fn publish(&self, publish: Publish<'_>) -> Result<(), ServerError> {
        let topic = publish.topic().path().split('/').collect::<Vec<_>>();

        if topic.len() != 4 || !topic[0].eq_ignore_ascii_case("command") {
            log::info!("Invalid topic name {:?}", topic);
            Err(ServerError::UnsupportedOperation)
        } else {
            let (app, device, command) = (topic[1], topic[2], topic[3]);

            log::info!("Sending command {:?} to {:?}/{:?}", command, app, device);

            let response = self
                .registry
                .get_device_and_gateways(&app, &device, Context::default())
                .await;

            match response {
                Ok(Some(device_gateways)) => {
                    let opts = CommandOptions {
                        application: app.to_string(),
                        device: device.to_string(),
                        command: command.to_string(),
                        timeout: None,
                    };

                    match drogue_cloud_integration_common::commands::process_command(
                        device_gateways.0,
                        device_gateways.1,
                        &self.sender,
                        None,
                        opts,
                        publish.payload().clone(),
                    )
                    .await
                    {
                        Ok(_) => Ok(()),
                        Err(e) => {
                            log::info!("Error sending command {:?}", e);
                            Err(ServerError::InternalError(format!(
                                "Error sending command {:?}",
                                e
                            )))
                        }
                    }
                }
                Ok(None) => Err(ServerError::NotAuthorized),
                Err(e) => {
                    log::info!("Error looking up registry info {:?}", e);
                    Err(ServerError::InternalError(format!(
                        "Error looking up registry info {:?}",
                        e
                    )))
                }
            }
        }
    }

    pub async fn closed(&self) -> Result<(), ServerError> {
        Ok(())
    }

    async fn run_stream(mut stream: Stream, sink: &mut Sink) -> Result<(), anyhow::Error> {
        let content_mode = stream.content_mode;

        log::debug!("Running stream - content-mode: {:?}", content_mode);

        // run event stream
        while let Some(event) = stream.event_stream.next().await {
            log::debug!("Event: {:?}", event);

            let mut event = event?;
            let topic = stream.topic.clone();

            match (&mut *sink, content_mode) {
                // MQTT v3.1
                (Sink::V3(sink), _) => {
                    let event = serde_json::to_vec(&event)?;
                    sink.publish(topic.clone(), event.into())
                        .send_at_most_once()
                }

                // MQTT v5 in structured mode
                (Sink::V5(sink), ContentMode::Structured) => {
                    let event = serde_json::to_vec(&event)?;
                    sink.publish(topic.clone(), event.into())
                        .properties(|p| {
                            p.content_type = Some(
                                "content-type: application/cloudevents+json; charset=utf-8".into(),
                            );
                            p.is_utf8_payload = Some(true);
                        })
                        .send_at_most_once()
                }

                // MQTT v5 in binary mode
                (Sink::V5(sink), ContentMode::Binary) => {
                    let (content_type, _, data) = event.take_data();
                    let builder = match data {
                        Some(Data::Binary(data)) => sink.publish(topic.clone(), data.into()),
                        Some(Data::String(data)) => sink.publish(topic.clone(), data.into()),
                        Some(Data::Json(data)) => {
                            sink.publish(topic.clone(), serde_json::to_vec(&data)?.into())
                        }
                        None => sink.publish(topic.clone(), Bytes::new()),
                    };

                    // convert attributes and extensions ...

                    builder
                        .properties(|p| {
                            for (k, v) in event.iter() {
                                p.user_properties.push((k.into(), v.to_string().into()));
                            }
                            p.content_type = content_type.map(Into::into);
                        })
                        // ... and send
                        .send_at_most_once()
                }
            }
            .map_err(|err| anyhow::anyhow!("Failed to send event: {}", err))?;

            log::debug!("Sent message - go back to sleep");
        }

        Ok(())
    }

    fn attach_app(&self, stream: Stream) {
        let topic = stream.topic.to_string();
        let mut sink = self.sink.clone();

        let f = async move {
            match Self::run_stream(stream, &mut sink).await {
                Ok(()) => log::debug!("Stream processor finished"),
                Err(err) => {
                    log::info!("Stream processor failed: {}", err);
                    sink.close();
                }
            }
        };

        let handle = ntex_rt::spawn(f);

        if let Ok(mut streams) = self.streams.lock() {
            streams.insert(topic, handle);
        }
    }

    fn detach(&self, topic: &str) -> bool {
        if let Ok(mut streams) = self.streams.lock() {
            if let Some(stream) = streams.remove(topic) {
                stream.abort();
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        log::debug!("Dropping session");
        if let Ok(mut streams) = self.streams.lock() {
            for (_, stream) in streams.iter() {
                stream.abort();
            }
            streams.clear();
        }
    }
}
