use crate::{
    service::{
        stream::{self, ContentMode, Stream},
        ServiceConfig,
    },
    CONNECTIONS_COUNTER,
};
use async_trait::async_trait;
use drogue_client::registry;
use drogue_cloud_endpoint_common::sender::UpstreamSender;
use drogue_cloud_event_common::stream::CustomAck;
use drogue_cloud_integration_common::{
    self,
    commands::CommandOptions,
    stream::{EventStream, EventStreamConfig},
};
use drogue_cloud_mqtt_common::{
    error::{PublishError, ServerError},
    mqtt::{self, *},
};
use drogue_cloud_service_api::{
    auth::user::{
        authz::{self, AuthorizationRequest, Permission},
        UserInformation,
    },
    kafka::{KafkaConfigExt, KafkaEventType},
};
use drogue_cloud_service_common::client::UserAuthClient;
use futures::lock::Mutex;
use ntex_mqtt::{types::QoS, v5};
use std::{collections::HashMap, num::NonZeroU32, sync::Arc};
use tokio::task::JoinHandle;

pub struct Session {
    pub config: ServiceConfig,
    pub user_auth: Option<Arc<UserAuthClient>>,

    pub sink: Sink,
    pub client_id: String,
    pub user: UserInformation,

    streams: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,

    pub sender: UpstreamSender,
    pub client: reqwest::Client,
    pub registry: registry::v1::Client,

    pub token: Option<String>,
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: ServiceConfig,
        user_auth: Option<Arc<UserAuthClient>>,
        sink: Sink,
        user: UserInformation,
        client_id: String,
        sender: UpstreamSender,
        client: reqwest::Client,
        registry: registry::v1::Client,
        token: Option<String>,
    ) -> Self {
        CONNECTIONS_COUNTER.inc();
        Session {
            config,
            user_auth,
            user,
            sink,
            client_id,
            streams: Arc::new(Mutex::new(HashMap::new())),
            sender,
            client,
            registry,
            token,
        }
    }

    async fn authorize(
        &self,
        application: String,
        user_auth: &Arc<UserAuthClient>,
        permission: Permission,
    ) -> Result<(), ()> {
        log::debug!(
            "Authorizing - user: {:?}, app: {}, permission: {:?}",
            self.user,
            application,
            permission
        );

        let response = user_auth
            .authorize(AuthorizationRequest {
                application,
                permission,
                user_id: self.user.user_id().map(ToString::to_string),
                roles: self.user.roles().clone(),
            })
            .await
            .map_err(|_| ())?;

        log::debug!("Outcome: {:?}", response);

        match response.outcome {
            authz::Outcome::Allow => Ok(()),
            authz::Outcome::Deny => Err(()),
        }
    }

    async fn subscribe_to(
        &self,
        id: Option<NonZeroU32>,
        original_topic: String,
        qos: QoS,
        content_mode: ContentMode,
    ) -> Result<QoS, v5::codec::SubscribeAckReason> {
        // split topic into path segments
        let topic = original_topic.split('/').collect::<Vec<_>>();

        // extract the shared named, which we use as kafka consumer group id
        let (group_id, topic) = match topic.as_slice() {
            ["$share", group_id, topic @ ..] => (Some(*group_id), topic),
            // keep incorrect topic prefix for a bit, to not break existing stuff
            ["$shared", group_id, topic @ ..] => (Some(*group_id), topic),
            other => {
                let group_id = if self.client_id.is_empty() {
                    None
                } else {
                    Some(self.client_id.as_str())
                };
                (group_id, other)
            }
        };

        // check QoS
        let qos = match qos {
            QoS::AtMostOnce => stream::QoS::AtMostOnce,
            QoS::AtLeastOnce | QoS::ExactlyOnce => stream::QoS::AtLeastOnce,
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
                self.authorize(app.to_string(), user_auth, Permission::Read)
                    .await
                    .map_err(|_| v5::codec::SubscribeAckReason::NotAuthorized)?;
            }
            None => {
                // authorization disabled ... nothing to do
            }
        }

        // find kafka info

        let app_res = self
            .registry
            .get_app(app)
            .await
            .map_err(|_| v5::codec::SubscribeAckReason::UnspecifiedError)?
            .ok_or(v5::codec::SubscribeAckReason::UnspecifiedError)?;

        // create stream

        let stream_config = EventStreamConfig {
            kafka: app_res
                .kafka_target(KafkaEventType::Events, &self.config.kafka)
                .map(|target| target.into())
                .map_err(|_| v5::codec::SubscribeAckReason::UnspecifiedError)?,
            consumer_group: group_id.map(|s| s.to_string()),
        };
        let event_stream = EventStream::<CustomAck>::new(stream_config).map_err(|err| {
            log::info!("Failed to subscribe to Kafka topic: {}", err);
            v5::codec::SubscribeAckReason::UnspecifiedError
        })?;

        // we started the stream, now hold on to it ...

        let stream = Stream {
            topic: original_topic.into(),
            qos,
            id,
            event_stream,
            content_mode,
        };

        self.attach_stream(stream).await;

        // done

        Ok(match qos {
            stream::QoS::AtMostOnce => QoS::AtMostOnce,
            stream::QoS::AtLeastOnce => QoS::AtLeastOnce,
        })
    }

    async fn attach_stream(&self, stream: Stream<'static>) {
        let topic = stream.topic.to_string();

        log::debug!("Attaching: {}", topic);

        let sink = self.sink.clone();
        let f = async move {
            stream.run(sink).await;
        };

        // spawn task

        let handle = ntex_rt::spawn(f);

        // remember it

        self.streams.lock().await.insert(topic, handle);
    }

    async fn detach(&self, topic: &str) -> bool {
        log::debug!("Detaching: {}", topic);

        let mut streams = self.streams.lock().await;
        if let Some(stream) = streams.remove(topic) {
            stream.abort();
            let result = stream.await;
            log::debug!("Stream result: {:?}", result);
            true
        } else {
            false
        }
    }
}

#[async_trait(?Send)]
impl mqtt::Session for Session {
    async fn publish(&self, publish: Publish<'_>) -> Result<(), PublishError> {
        let topic = publish.topic().path().split('/').collect::<Vec<_>>();

        if topic.len() != 4 || !topic[0].eq_ignore_ascii_case("command") {
            log::info!("Invalid topic name {:?}", topic);
            Err(PublishError::UnsupportedOperation)
        } else {
            let (app, device, command) = (topic[1], topic[2], topic[3]);

            log::info!(
                "Request to send command {:?} to {:?}/{:?}",
                command,
                app,
                device
            );

            if let Some(user_auth) = &self.user_auth {
                self.authorize(app.to_string(), user_auth, Permission::Write)
                    .await
                    .map_err(|_| PublishError::NotAuthorized)?;
            }

            let response = futures::try_join!(
                self.registry.get_app(&app),
                self.registry.get_device_and_gateways(&app, &device)
            );

            match response {
                Ok((Some(application), Some(device_gateways))) => {
                    let opts = CommandOptions {
                        application: app.to_string(),
                        device: device.to_string(),
                        command: command.to_string(),
                        content_type: None,
                    };

                    match drogue_cloud_integration_common::commands::process_command(
                        application,
                        device_gateways.0,
                        device_gateways.1,
                        &self.sender,
                        self.client.clone(),
                        opts,
                        bytes::Bytes::from(publish.payload().to_vec()),
                    )
                    .await
                    {
                        Ok(_) => Ok(()),
                        Err(e) => {
                            log::info!("Error sending command {:?}", e);
                            Err(PublishError::InternalError(format!(
                                "Error sending command {:?}",
                                e
                            )))
                        }
                    }
                }
                Ok(_) => Err(PublishError::NotAuthorized),
                Err(e) => {
                    log::info!("Error looking up registry info {:?}", e);
                    Err(PublishError::InternalError(format!(
                        "Error looking up registry info {:?}",
                        e
                    )))
                }
            }
        }
    }

    async fn subscribe(&self, subscribe: Subscribe<'_>) -> Result<(), ServerError> {
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
                .subscribe_to(id, sub.topic().to_string(), sub.qos(), content_mode)
                .await;
            log::debug!("Subscribing to: {:?} -> {:?}", sub.topic(), res);
            match res {
                Ok(qos) => sub.confirm(qos),
                Err(reason) => sub.fail(reason),
            }
        }

        Ok(())
    }

    async fn unsubscribe(&self, unsubscribe: Unsubscribe<'_>) -> Result<(), ServerError> {
        for unsub in unsubscribe.into_iter() {
            let topic = unsub.topic();
            log::debug!("Unsubscribe: {:?}", topic);
            if !self.detach(topic.as_ref()).await {
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

    async fn closed(&self, reason: CloseReason) -> Result<(), ServerError> {
        log::info!("Connection closed: {:?}", reason);
        CONNECTIONS_COUNTER.dec();
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        log::debug!("Dropping session");
        let streams = self.streams.clone();
        ntex_rt::spawn(async move {
            log::debug!("Dropping streams");
            let mut streams = streams.lock().await;

            for (_, stream) in streams.drain() {
                stream.abort();
            }
        });
    }
}
