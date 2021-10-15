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
use ntex::util::{ByteString, Bytes};
use ntex_mqtt::{types::QoS, v5};

const TOPIC_COMMAND_INBOX: &str = "command/inbox";
const TOPIC_COMMAND_INBOX_PATTERN: &str = "command/inbox/#";

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
        }
    }

    async fn run_commands(&self) {
        let device_id = self.device_id.clone();
        let mut rx = self.commands.subscribe(device_id.clone()).await;
        let sink = self.sink.clone();

        ntex::rt::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match Self::send_command(&sink, cmd).await {
                    Ok(_) => {
                        log::debug!("Command sent to device subscription {:?}", device_id);
                    }
                    Err(e) => {
                        log::error!("Failed to send a command to device subscription {:?}", e);
                    }
                }
            }
        });
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
        for mut sub in sub {
            if sub.topic() == TOPIC_COMMAND_INBOX_PATTERN {
                self.run_commands().await;
                log::debug!(
                    "Device '{:?}' subscribed to receive commands",
                    self.device_id
                );
                sub.confirm(QoS::AtLeastOnce);
            } else {
                log::info!("Subscribing to topic {:?} not allowed", sub.topic());
                sub.fail(v5::codec::SubscribeAckReason::UnspecifiedError);
            }
        }

        Ok(())
    }

    async fn unsubscribe(
        &self,
        _unsubscribe: Unsubscribe<'_>,
    ) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        // FIXME: any unsubscribe get we, we treat as disconnecting from the command inbox
        self.commands.unsubscribe(&self.device_id).await;
        Ok(())
    }

    async fn closed(&self) -> Result<(), drogue_cloud_mqtt_common::error::ServerError> {
        self.commands.unsubscribe(&self.device_id).await;
        Ok(())
    }
}
