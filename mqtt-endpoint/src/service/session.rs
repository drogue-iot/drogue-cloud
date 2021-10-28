use async_trait::async_trait;
use drogue_client::registry;
use drogue_cloud_endpoint_common::{
    command::{Command, Commands},
    sender::{self, DownstreamSender, PublishOutcome, Publisher},
    sink::Sink,
};
use drogue_cloud_mqtt_common::{
    error::PublishError,
    mqtt::{self, *},
};
use drogue_cloud_service_common::Id;
use futures::lock::Mutex;
use ntex::util::{ByteString, Bytes};
use ntex_mqtt::{types::QoS, v5};
use std::sync::Arc;

const TOPIC_COMMAND_INBOX: &str = "command/inbox";
const TOPIC_COMMAND_INBOX_PATTERN: &str = "command/inbox/#";

pub struct InboxReader {
    device_id: Id,
    commands: Commands,
}

impl InboxReader {
    async fn new(device_id: Id, commands: Commands, sink: mqtt::Sink) -> Self {
        let mut rx = commands.subscribe(device_id.clone()).await;

        let id = device_id.clone();

        ntex::rt::spawn(async move {
            log::debug!("Starting inbox command loop: {:?}", id);
            while let Some(cmd) = rx.recv().await {
                match Self::send_command(&sink, cmd).await {
                    Ok(_) => {
                        log::debug!("Command sent to device subscription {:?}", id);
                    }
                    Err(e) => {
                        log::error!("Failed to send a command to device subscription {:?}", e);
                    }
                }
            }
            log::debug!("Exiting inbox command loop: {:?}", id);
        });

        Self {
            device_id,
            commands,
        }
    }

    async fn send_command(sink: &mqtt::Sink, cmd: Command) -> Result<(), String> {
        let topic = ByteString::from(format!("{}/{}", TOPIC_COMMAND_INBOX, cmd.command));

        let payload = match cmd.payload {
            Some(payload) => Bytes::from(payload),
            None => Bytes::new(),
        };

        match sink {
            mqtt::Sink::V3(sink) => match sink.publish(topic, payload).send_at_least_once().await {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
            mqtt::Sink::V5(sink) => match sink.publish(topic, payload).send_at_least_once().await {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        }
    }
}

impl Drop for InboxReader {
    fn drop(&mut self) {
        log::debug!("Dropping inbox reader for {:?}", self.device_id);

        // unsubscribe from commands
        let device_id = self.device_id.clone();
        let commands = self.commands.clone();
        ntex::rt::spawn(async move {
            commands.unsubscribe(&device_id).await;
        });
    }
}

#[derive(Clone)]
pub struct Session<S>
where
    S: Sink,
{
    pub sender: DownstreamSender<S>,
    pub application: registry::v1::Application,
    pub device_id: Id,
    pub commands: Commands,
    sink: mqtt::Sink,
    inbox_reader: Arc<Mutex<Option<InboxReader>>>,
}

impl<S> Session<S>
where
    S: Sink,
{
    pub fn new(
        sender: DownstreamSender<S>,
        sink: mqtt::Sink,
        application: registry::v1::Application,
        device_id: Id,
        commands: Commands,
    ) -> Self {
        Self {
            sender,
            sink,
            application,
            device_id,
            commands,
            inbox_reader: Default::default(),
        }
    }

    async fn subscribe_inbox(&self) {
        let mut reader = self.inbox_reader.lock().await;

        match reader.as_ref() {
            Some(_) => {
                log::info!("Already subscribed to command inbox");
            }
            None => {
                reader.replace(
                    InboxReader::new(
                        self.device_id.clone(),
                        self.commands.clone(),
                        self.sink.clone(),
                    )
                    .await,
                );
                log::debug!(
                    "Device '{:?}' subscribed to receive commands",
                    self.device_id
                );
            }
        }
    }
}

#[async_trait(?Send)]
impl<S> mqtt::Session for Session<S>
where
    S: Sink,
{
    async fn publish(&self, publish: Publish<'_>) -> Result<(), PublishError> {
        let channel = publish.topic().path();
        let id = self.device_id.clone();

        match self
            .sender
            .publish(
                sender::Publish {
                    channel: channel.into(),
                    application: &self.application,
                    device_id: id.device_id,
                    options: Default::default(),
                },
                publish.payload(),
            )
            .await
        {
            Ok(PublishOutcome::Accepted) => Ok(()),
            Ok(PublishOutcome::Rejected) => Err(PublishError::UnspecifiedError),
            Ok(PublishOutcome::QueueFull) => Err(PublishError::QuotaExceeded),
            Err(err) => Err(PublishError::InternalError(err.to_string())),
        }
    }

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
            match sub.topic().as_ref() {
                TOPIC_COMMAND_INBOX_PATTERN => {
                    log::info!("Subscribing to device command inbox: {:?}", self.device_id);
                    self.subscribe_inbox().await;
                    sub.confirm(QoS::AtLeastOnce);
                }
                _ => {
                    log::info!("Subscribing to topic {:?} not allowed", sub.topic());
                    sub.fail(v5::codec::SubscribeAckReason::UnspecifiedError);
                }
            }
        }

        Ok(())
    }

    async fn unsubscribe(
        &self,
        unsubscribe: Unsubscribe<'_>,
    ) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        for mut unsub in unsubscribe {
            match unsub.topic().as_ref() {
                TOPIC_COMMAND_INBOX_PATTERN => {
                    if self.inbox_reader.lock().await.take().is_some() {
                        unsub.success();
                    } else {
                        log::info!(
                            "Tried to unsubscribe from not-subscribed inbox reader: {:?}",
                            self.device_id
                        );
                        unsub.fail(v5::codec::UnsubscribeAckReason::NoSubscriptionExisted);
                    }
                }
                _ => {
                    log::info!(
                        "Tried to unsubscribe from not-subscribed topic: {}",
                        unsub.topic()
                    );
                    unsub.fail(v5::codec::UnsubscribeAckReason::NoSubscriptionExisted);
                }
            }
        }

        Ok(())
    }

    async fn closed(&self) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        self.inbox_reader.lock().await.take();
        Ok(())
    }
}
