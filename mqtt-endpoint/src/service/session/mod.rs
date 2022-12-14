mod cache;
mod dialect;
mod disconnect;
mod inbox;

use self::disconnect::*;
use crate::{
    auth::DeviceAuthenticator,
    config::EndpointConfig,
    service::session::dialect::{
        DefaultTopicParser, ParsedSubscribeTopic, SubscriptionTopicEncoder,
    },
    CONNECTIONS_COUNTER,
};
use async_trait::async_trait;
use cache::DeviceCache;
use drogue_client::registry;
use drogue_cloud_endpoint_common::{
    command::{CommandFilter, Commands},
    sender::{
        self, DownstreamSender, PublishOptions, PublishOutcome, Publisher, ToPublishId,
        DOWNSTREAM_EVENTS_COUNTER,
    },
};
use drogue_cloud_mqtt_common::{
    error::{PublishError, ServerError},
    mqtt::{self, *},
};
use drogue_cloud_service_api::{
    auth::device::authn::GatewayOutcome, services::device_state::DeleteOptions,
};
use drogue_cloud_service_common::{
    state::{State, StateHandle},
    Id,
};
use futures::{lock::Mutex, TryFutureExt};
use inbox::InboxSubscription;
use ntex_mqtt::{
    types::QoS,
    v5::codec::{self, DisconnectReasonCode},
};
use std::{
    cell::Cell,
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};
use tracing::instrument;

pub struct Session {
    sender: DownstreamSender,
    application: registry::v1::Application,
    device: Arc<registry::v1::Device>,
    dialect: registry::v1::MqttDialect,
    commands: Commands,
    auth: DeviceAuthenticator,
    sink: Sink,
    inbox_reader: Arc<Mutex<HashMap<String, InboxSubscription>>>,
    device_cache: DeviceCache<registry::v1::Device>,
    id: Id,
    handle: Cell<Option<StateHandle>>,
    disconnect: DisconnectHandle,
}

impl Session {
    pub fn new(
        config: &EndpointConfig,
        auth: DeviceAuthenticator,
        sender: DownstreamSender,
        sink: Sink,
        application: registry::v1::Application,
        dialect: registry::v1::MqttDialect,
        device: registry::v1::Device,
        commands: Commands,
        state: State,
    ) -> Self {
        let id = Id::new(
            application.metadata.name.clone(),
            device.metadata.name.clone(),
        );
        let device_cache = DeviceCache::new(config.cache_size, config.cache_duration);
        CONNECTIONS_COUNTER.inc();

        let (handle, watcher) = state.split();

        {
            let sink = Arc::new(sink.clone());
            let id = id.clone();
            let watcher = async move {
                let cause = watcher.lost().await;
                log::info!("Lost device state: {id:?} - cause: {cause:?}");
                match sink.as_ref() {
                    Sink::V3(sink) => sink.close(),
                    Sink::V5(sink) => sink.close_with_reason(codec::Disconnect {
                        reason_code: DisconnectReasonCode::SessionTakenOver,
                        session_expiry_interval_secs: None,
                        server_reference: None,
                        reason_string: None,
                        user_properties: vec![],
                    }),
                }
            };

            ntex_rt::spawn(watcher);
        }

        Self {
            auth,
            sender,
            sink,
            application,
            device: Arc::new(device),
            dialect,
            commands,
            inbox_reader: Default::default(),
            device_cache,
            id,
            handle: Cell::new(Some(handle)),
            disconnect: DisconnectHandle::new(),
        }
    }

    /// Subscribe to a command inbox
    async fn subscribe_inbox<F>(
        &self,
        topic_filter: F,
        filter: CommandFilter,
        encoder: SubscriptionTopicEncoder,
    ) where
        F: Into<String>,
    {
        let topic_filter = topic_filter.into();
        let mut reader = self.inbox_reader.lock().await;

        let entry = reader.entry(topic_filter);

        match entry {
            Entry::Occupied(_) => {
                log::info!("Already subscribed to command inbox");
            }
            Entry::Vacant(entry) => {
                log::debug!("Subscribe device '{:?}' to receive commands", self.id);
                let subscription = InboxSubscription::new(
                    filter,
                    self.commands.clone(),
                    self.sink.clone(),
                    encoder,
                )
                .await;
                entry.insert(subscription);
            }
        }
    }

    #[instrument(level = "debug", skip(self), fields(self.id = ?self.id), err)]
    async fn eval_device(
        &self,
        publish: &Publish<'_>,
    ) -> Result<(String, Arc<registry::v1::Device>), PublishError> {
        match self.dialect.parse_publish(publish.topic().path()) {
            Ok(topic) => match topic.device {
                None => Ok((topic.channel.to_string(), self.device.clone())),
                Some(device) if device == self.id.device_id => {
                    Ok((topic.channel.to_string(), self.device.clone()))
                }
                Some(device) => self
                    .device_cache
                    .fetch(device, |device| {
                        self.auth
                            .authorize_as(
                                &self.application.metadata.name,
                                &self.device.metadata.name,
                                device,
                            )
                            .map_ok(|result| match result.outcome {
                                GatewayOutcome::Pass { r#as } => Some(r#as),
                                _ => None,
                            })
                    })
                    .await
                    .map(|r| (topic.channel.to_string(), r)),
            },
            Err(_) => Err(PublishError::TopicNameInvalid),
        }
    }
}

#[async_trait(? Send)]
impl mqtt::Session for Session {
    #[instrument(level = "debug", skip(self), fields(self.id = ?self.id), err)]
    async fn publish(&self, publish: Publish<'_>) -> Result<(), PublishError> {
        let _lock = self.disconnect.ensure().await?;

        let content_type = publish
            .properties()
            .and_then(|p| p.content_type.as_ref())
            .map(|s| s.to_string());

        let (channel, device) = self.eval_device(&publish).await?;

        log::debug!(
            "Publish as {} / {} ({}) to {}",
            self.application.metadata.name,
            device.metadata.name,
            self.device.metadata.name,
            channel
        );

        match self
            .sender
            .publish(
                sender::Publish {
                    channel: channel.to_string(),
                    application: &self.application,
                    device: device.metadata.to_id(),
                    sender: self.device.metadata.to_id(),
                    options: PublishOptions {
                        content_type,
                        ..Default::default()
                    },
                },
                publish.payload(),
            )
            .await
        {
            Ok(PublishOutcome::Accepted) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["mqtt", "Accepted"])
                    .inc();
                Ok(())
            }
            Ok(PublishOutcome::Rejected) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["mqtt", "Rejected"])
                    .inc();
                Err(PublishError::UnspecifiedError)
            }
            Ok(PublishOutcome::QueueFull) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["mqtt", "QueueFull"])
                    .inc();
                Err(PublishError::QuotaExceeded)
            }
            Err(err) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["mqtt", "Error"])
                    .inc();
                Err(PublishError::InternalError(err.to_string()))
            }
        }
    }

    #[instrument(skip(self),fields(self.id = ?self.id))]
    async fn subscribe(&self, sub: Subscribe<'_>) -> Result<(), ServerError> {
        if sub.id().is_some() {
            log::info!("Rejecting request with subscription IDs");
            for mut sub in sub {
                sub.fail(codec::SubscribeAckReason::SubscriptionIdentifiersNotSupported);
            }
            return Ok(());
        }

        for mut sub in sub {
            log::debug!("Checking subscription request: {sub:?}");

            match self.dialect.parse_subscribe(sub.topic()) {
                Ok(ParsedSubscribeTopic { filter, encoder }) => {
                    self.subscribe_inbox(
                        sub.topic().to_string(),
                        filter.into_command_filter(&self.id),
                        encoder,
                    )
                    .await;
                    sub.confirm(QoS::AtMostOnce);
                }
                Err(err) => {
                    log::info!("Subscribing to topic {:?} not allowed: {err}", sub.topic());
                    sub.fail(codec::SubscribeAckReason::UnspecifiedError);
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self),fields(self.id = ?self.id))]
    async fn unsubscribe(&self, unsubscribe: Unsubscribe<'_>) -> Result<(), ServerError> {
        let mut subscriptions = self.inbox_reader.lock().await;

        for mut unsub in unsubscribe {
            match subscriptions.remove(unsub.topic().as_ref()) {
                Some(subscription) => {
                    subscription.close().await;
                    unsub.success();
                }
                None => {
                    log::info!(
                        "Tried to unsubscribe from not-subscribed inbox reader: {:?}",
                        self.device.metadata.name
                    );
                    unsub.fail(codec::UnsubscribeAckReason::NoSubscriptionExisted);
                }
            }
        }

        Ok(())
    }

    #[instrument(
        skip(self),
        fields(self.id = ?self.id),
        err(Debug)
    )]
    async fn disconnect(&self, disconnect: Disconnect<'_>) -> Result<(), ServerError> {
        self.disconnect.disconnected(disconnect).await?;

        Ok(())
    }

    #[instrument(
        skip(self),
        fields(self.id = ?self.id),
        err(Debug)
    )]
    async fn closed(&self, reason: CloseReason) -> Result<(), ServerError> {
        log::info!("Connection closed ({:?}): {:?}", self.id, reason);

        // lock and check lwt flag
        let skip_lwt = self.disconnect.close().await;

        if let Some(mut handle) = self.handle.take() {
            handle.delete(DeleteOptions { skip_lwt }).await;
        }

        for (_, v) in self.inbox_reader.lock().await.drain() {
            v.close().await;
        }

        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(mut handle) = self.handle.take() {
            log::warn!("Late handling session state deletion");
            ntex_rt::spawn(async move {
                handle.delete(DeleteOptions { skip_lwt: false }).await;
            });
        }

        CONNECTIONS_COUNTER.dec();
    }
}
