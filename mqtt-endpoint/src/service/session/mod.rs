mod cache;
mod inbox;

use crate::{auth::DeviceAuthenticator, config::EndpointConfig, CONNECTIONS_COUNTER};
use async_trait::async_trait;
use cache::DeviceCache;
use drogue_client::registry;
use drogue_cloud_endpoint_common::{
    command::{CommandFilter, Commands},
    sender::{
        self, DownstreamSender, PublishOptions, PublishOutcome, Publisher,
        DOWNSTREAM_EVENTS_COUNTER,
    },
    sink::Sink,
};
use drogue_cloud_mqtt_common::{
    error::PublishError,
    mqtt::{self, *},
};
use drogue_cloud_service_api::auth::device::authn::GatewayOutcome;
use drogue_cloud_service_common::Id;
use futures::{lock::Mutex, TryFutureExt};
use inbox::InboxSubscription;
use ntex_mqtt::{types::QoS, v5};
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};
use tracing::instrument;

#[derive(Clone)]
pub struct Session<S>
where
    S: Sink,
{
    sender: DownstreamSender<S>,
    application: registry::v1::Application,
    device: Arc<registry::v1::Device>,
    commands: Commands,
    auth: DeviceAuthenticator,
    sink: mqtt::Sink,
    inbox_reader: Arc<Mutex<HashMap<String, InboxSubscription>>>,
    device_cache: DeviceCache<registry::v1::Device>,
    id: Id,
}

impl<S> Session<S>
where
    S: Sink,
{
    pub fn new(
        config: &EndpointConfig,
        auth: DeviceAuthenticator,
        sender: DownstreamSender<S>,
        sink: mqtt::Sink,
        application: registry::v1::Application,
        device: registry::v1::Device,
        commands: Commands,
    ) -> Self {
        let id = Id::new(
            application.metadata.name.clone(),
            device.metadata.name.clone(),
        );
        let device_cache = cache::DeviceCache::new(config.cache_size, config.cache_duration);
        CONNECTIONS_COUNTER.inc();
        Self {
            auth,
            sender,
            sink,
            application,
            device: Arc::new(device),
            commands,
            inbox_reader: Default::default(),
            device_cache,
            id,
        }
    }

    async fn subscribe_inbox<F: Into<String>>(
        &self,
        topic_filter: F,
        filter: CommandFilter,
        force_device: bool,
    ) {
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
                    force_device,
                )
                .await;
                entry.insert(subscription);
            }
        }
    }

    #[instrument(skip(self), fields(self.id = ?self.id))]
    async fn eval_device(
        &self,
        publish: &Publish<'_>,
    ) -> Result<(String, Arc<registry::v1::Device>), PublishError> {
        let topic = publish.topic().path().split('/').collect::<Vec<_>>();
        log::debug!("Topic: {:?}", topic);

        match topic.as_slice() {
            [channel] => Ok((channel.to_string(), self.device.clone())),
            [channel, as_device] => self
                .device_cache
                .fetch(as_device, |as_device| {
                    self.auth
                        .authorize_as(
                            &self.application.metadata.name,
                            &self.device.metadata.name,
                            as_device,
                        )
                        .map_ok(|result| match result.outcome {
                            GatewayOutcome::Pass { r#as } => Some(r#as),
                            _ => None,
                        })
                })
                .await
                .map(|r| (channel.to_string(), r)),
            _ => Err(PublishError::TopicNameInvalid),
        }
    }
}

#[async_trait(?Send)]
impl<S> mqtt::Session for Session<S>
where
    S: Sink,
{
    #[instrument(skip(self),fields(self.id = ?self.id),err)]
    async fn publish(&self, publish: Publish<'_>) -> Result<(), PublishError> {
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
                    device_id: device.metadata.name.clone(),
                    sender_id: self.device.metadata.name.clone(),
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
    async fn subscribe(
        &self,
        sub: Subscribe<'_>,
    ) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        if sub.id().is_some() {
            log::info!("Rejecting request with subscription IDs");
            for mut sub in sub {
                sub.fail(v5::codec::SubscribeAckReason::SubscriptionIdentifiersNotSupported);
            }
            return Ok(());
        }

        for mut sub in sub {
            match sub.topic().split('/').collect::<Vec<_>>().as_slice() {
                ["command", "inbox", "#"] | ["command", "inbox", "+", "#"] => {
                    self.subscribe_inbox(
                        sub.topic().to_string(),
                        CommandFilter::wildcard(self.id.app_id.clone(), self.id.device_id.clone()),
                        false,
                    )
                    .await;
                    sub.confirm(QoS::AtMostOnce);
                }
                ["command", "inbox", "", "#"] => {
                    self.subscribe_inbox(
                        sub.topic().to_string(),
                        CommandFilter::device(self.id.app_id.clone(), self.id.device_id.clone()),
                        false,
                    )
                    .await;
                    sub.confirm(QoS::AtMostOnce);
                }
                ["command", "inbox", device, "#"] => {
                    self.subscribe_inbox(
                        sub.topic().to_string(),
                        CommandFilter::proxied_device(
                            self.id.app_id.clone(),
                            self.id.device_id.clone(),
                            *device,
                        ),
                        true,
                    )
                    .await;
                    sub.confirm(QoS::AtMostOnce);
                }
                _ => {
                    log::info!("Subscribing to topic {:?} not allowed", sub.topic());
                    sub.fail(v5::codec::SubscribeAckReason::UnspecifiedError);
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self),fields(self.id = ?self.id))]
    async fn unsubscribe(
        &self,
        unsubscribe: Unsubscribe<'_>,
    ) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
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
                    unsub.fail(v5::codec::UnsubscribeAckReason::NoSubscriptionExisted);
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self),fields(self.id = ?self.id))]
    async fn closed(
        &self,
        reason: CloseReason,
    ) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        log::info!("Connection closed ({:?}): {:?}", self.id, reason);
        for (_, v) in self.inbox_reader.lock().await.drain() {
            v.close().await;
        }
        CONNECTIONS_COUNTER.dec();
        Ok(())
    }
}
