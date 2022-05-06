mod cache;
mod dialect;
mod inbox;

use crate::{
    auth::DeviceAuthenticator, config::EndpointConfig,
    service::session::dialect::DefaultTopicParser, CONNECTIONS_COUNTER,
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
    v5::{
        self,
        codec::{self, DisconnectReasonCode},
    },
};
use std::{
    cell::Cell,
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
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
    skip_lwt: Arc<AtomicBool>,
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
                watcher.lost().await;
                log::info!("Lost device state: {id:?}");
                match sink.as_ref() {
                    Sink::V3(sink) => sink.force_close(),
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
            skip_lwt: Arc::new(AtomicBool::new(false)),
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

    #[instrument(level = "debug", skip(self), fields(self.id = ?self.id), err)]
    async fn eval_device(
        &self,
        publish: &Publish<'_>,
    ) -> Result<(String, Arc<registry::v1::Device>), PublishError> {
        match self.dialect.parse_publish(publish.topic().path()) {
            Ok(topic) => match topic.device {
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
                None => Ok((topic.channel.to_string(), self.device.clone())),
            },
            Err(_) => Err(PublishError::TopicNameInvalid),
        }
    }
}

#[async_trait(? Send)]
impl mqtt::Session for Session {
    #[instrument(level = "debug", skip(self), fields(self.id = ?self.id), err)]
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
                    unsub.fail(v5::codec::UnsubscribeAckReason::NoSubscriptionExisted);
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
        match disconnect.reason_code() {
            DisconnectReasonCode::NormalDisconnection => {
                log::debug!("Normal disconnect, skipping LWT");
                self.skip_lwt.store(true, Ordering::Release);
            }
            _ => {}
        }

        Ok(())
    }

    #[instrument(
        skip(self),
        fields(self.id = ?self.id),
        err(Debug)
    )]
    async fn closed(&self, reason: CloseReason) -> Result<(), ServerError> {
        log::info!("Connection closed ({:?}): {:?}", self.id, reason);

        CONNECTIONS_COUNTER.dec();

        // check if we need to skip the LWT
        let skip_lwt = self.skip_lwt.load(Ordering::Acquire);

        if let Some(mut handle) = self.handle.take() {
            handle.delete(DeleteOptions { skip_lwt }).await;
        }

        for (_, v) in self.inbox_reader.lock().await.drain() {
            v.close().await;
        }

        Ok(())
    }
}
