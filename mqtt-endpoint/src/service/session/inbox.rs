use drogue_cloud_endpoint_common::command::{
    Command, CommandFilter, Commands, Subscription, SubscriptionHandle,
};
use drogue_cloud_mqtt_common::mqtt;
use ntex::util::{ByteString, Bytes};

pub struct InboxSubscription {
    filter: CommandFilter,
    handle: Option<InboxSubscriptionHandle>,
}

struct InboxSubscriptionHandle {
    handle: SubscriptionHandle,
    commands: Commands,
}

impl InboxSubscriptionHandle {
    async fn close(self) {
        log::debug!("Unsubscribe from commands: {:?}", self.handle);
        self.commands.unsubscribe(self.handle).await;
    }
}

impl InboxSubscription {
    pub async fn new(
        filter: CommandFilter,
        commands: Commands,
        sink: mqtt::Sink,
        force_device: bool,
    ) -> Self {
        // TODO: try to reduce cloning

        let Subscription {
            mut receiver,
            handle,
        } = commands.subscribe(filter.clone()).await;

        let sub_filter = filter.clone();

        ntex::rt::spawn(async move {
            log::debug!("Starting inbox command loop: {:?}", sub_filter);
            while let Some(cmd) = receiver.recv().await {
                match Self::send_command(&sink, force_device, cmd).await {
                    Ok(_) => {
                        log::debug!("Command sent to device subscription {:?}", sub_filter);
                    }
                    Err(e) => {
                        log::error!("Failed to send a command to device subscription {:?}", e);
                    }
                }
            }
            log::debug!("Exiting inbox command loop: {:?}", sub_filter);
        });

        Self {
            filter,
            handle: Some(InboxSubscriptionHandle { handle, commands }),
        }
    }

    async fn send_command(
        sink: &mqtt::Sink,
        force_device: bool,
        cmd: Command,
    ) -> Result<(), String> {
        let topic = if force_device || cmd.address.gateway_id != cmd.address.device_id {
            format!("command/inbox/{}/{}", cmd.address.device_id, cmd.command)
        } else {
            format!("command/inbox//{}", cmd.command)
        };

        let topic = ByteString::from(topic);

        let payload = match cmd.payload {
            Some(payload) => Bytes::from(payload),
            None => Bytes::new(),
        };

        match sink {
            mqtt::Sink::V3(sink) => match sink.publish(topic, payload).send_at_most_once() {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
            mqtt::Sink::V5(sink) => match sink.publish(topic, payload).send_at_most_once() {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        }
    }

    pub async fn close(mut self) {
        if let Some(handle) = self.handle.take() {
            log::debug!("Closing inbox reader for {:?}", self.filter);
            handle.close().await;
        } else {
            log::debug!("Inbox reader for {:?} already closed", self.filter);
        }
    }
}

impl Drop for InboxSubscription {
    fn drop(&mut self) {
        // unsubscribe from commands
        if let Some(handle) = self.handle.take() {
            log::debug!("Dropping inbox reader for {:?}", self.filter);
            ntex::rt::spawn(async move {
                handle.close().await;
            });
        } else {
            log::debug!(
                "Dropping inbox reader for {:?} (already closed)",
                self.filter
            );
        }
    }
}
