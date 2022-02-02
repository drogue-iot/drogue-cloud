use crate::{
    service::{
        stream::{ContentMode, Stream},
        ServiceConfig,
    },
    CONNECTIONS_COUNTER,
};
use async_trait::async_trait;
use cloudevents::Data;
use drogue_client::{openid::OpenIdTokenProvider, registry};
use drogue_cloud_endpoint_common::{sender::UpstreamSender, sink::Sink as SenderSink};
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
use futures::StreamExt;
use ntex::util::Bytes;
use ntex_mqtt::{types::QoS, v5};
use std::{collections::HashMap, num::NonZeroU32, sync::Arc};
use tokio::task::JoinHandle;

pub struct Session<S: SenderSink> {
    pub config: ServiceConfig,
    pub user_auth: Option<Arc<UserAuthClient>>,

    pub sink: Sink,
    pub client_id: String,
    pub user: UserInformation,

    streams: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,

    pub sender: UpstreamSender<S>,
    pub client: reqwest::Client,
    pub registry: registry::v1::Client<Option<OpenIdTokenProvider>>,

    pub token: Option<String>,
}

impl<S> Session<S>
where
    S: SenderSink,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: ServiceConfig,
        user_auth: Option<Arc<UserAuthClient>>,
        sink: Sink,
        user: UserInformation,
        client_id: String,
        sender: UpstreamSender<S>,
        client: reqwest::Client,
        registry: registry::v1::Client<Option<OpenIdTokenProvider>>,
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

        let event_stream = EventStream::new(EventStreamConfig {
            kafka: app_res
                .kafka_config(KafkaEventType::Events, &self.config.kafka)
                .map_err(|_| v5::codec::SubscribeAckReason::UnspecifiedError)?,
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
            event_stream,
            content_mode,
        };

        self.attach_app(stream).await;

        // done

        Ok(QoS::AtMostOnce)
    }

    async fn run_stream(mut stream: Stream<'_>, sink: &mut Sink) -> Result<(), anyhow::Error> {
        let content_mode = stream.content_mode;
        let sub_id = stream.id.map(|id| vec![id]);

        log::debug!(
            "Running stream - content-mode: {:?}, subscription-ids: {:?}",
            content_mode,
            sub_id
        );

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
                            p.content_type =
                                Some("application/cloudevents+json; charset=utf-8".into());
                            p.is_utf8_payload = Some(true);
                            p.subscription_ids = sub_id.clone();
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
                            p.subscription_ids = sub_id.clone();
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

    async fn attach_app(&self, stream: Stream<'static>) {
        let topic = stream.topic.to_string();

        log::debug!("Attaching: {}", topic);

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
impl<S> mqtt::Session for Session<S>
where
    S: SenderSink,
{
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

impl<S> Drop for Session<S>
where
    S: SenderSink,
{
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
