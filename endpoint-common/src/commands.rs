use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use cloudevents::{AttributesReader, Event};
use drogue_cloud_service_common::Id;
use std::convert::TryFrom;
use tokio::sync::mpsc::{channel, Receiver, Sender};

/// Represents command
#[derive(Clone, Debug)]
pub struct Command {
    pub device_id: Id,
    pub command: String,
    pub payload: Option<String>,
}

impl Command {
    /// Create a new scoped Command
    pub fn new<C: ToString>(device_id: Id, command: C, payload: Option<String>) -> Self {
        Self {
            device_id,
            command: command.to_string(),
            payload,
        }
    }
}

impl TryFrom<Event> for Command {
    type Error = ();

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        match Id::from_event(&event) {
            Some(device_id) => Ok(Command::new(
                device_id,
                event.subject().unwrap().to_string(),
                String::try_from(event.data().unwrap().clone()).ok(),
            )),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Commands {
    pub devices: Arc<Mutex<HashMap<Id, Sender<Command>>>>,
}

impl Default for Commands {
    fn default() -> Self {
        Self::new()
    }
}

impl Commands {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn send(&self, msg: Command) -> Result<(), String> {
        let device = { self.devices.lock().unwrap().get(&msg.device_id).cloned() };
        if let Some(sender) = device {
            match sender.send(msg.clone()).await {
                Ok(_) => {
                    log::debug!(
                        "Command {:?} sent to device {:?}",
                        msg.command.clone(),
                        msg.device_id
                    );
                    Ok(())
                }
                Err(e) => {
                    log::error!("Failed to send a command {:?}", e);
                    Err(e.to_string())
                }
            }
        } else {
            log::debug!(
                "Failed to route command: No device {:?} found on this endpoint!",
                msg.device_id
            );
            Ok(())
        }
    }

    pub fn subscribe(&self, device_id: Id) -> Receiver<Command> {
        let (tx, rx) = channel(32);
        let mut devices = self.devices.lock().unwrap();
        devices.insert(device_id.clone(), tx);
        log::debug!("Device {:?} subscribed to receive commands", device_id);
        rx
    }

    pub fn unsubscribe(&self, device_id: Id) {
        let mut devices = self.devices.lock().unwrap();
        devices.remove(&device_id);
        log::debug!(
            "Device {:?} unsubscribed from receiving commands",
            device_id
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_timeout() {
        let _ = env_logger::try_init();
        let id = Id::new("test-timeout", "test");

        let commands = Commands::new();

        let mut receiver = commands.subscribe(id.clone());

        let handle = tokio::spawn(async move {
            let cmd = timeout(Duration::from_secs(1), receiver.recv()).await;
            log::info!("Received {:?}", cmd);
            assert_eq!(cmd.unwrap().unwrap().command, "test".to_string());
            let cmd2 = timeout(Duration::from_secs(1), receiver.recv()).await;
            log::info!("Received {:?}", cmd2);
            assert_eq!(cmd2.is_err(), true);
        });

        commands
            .send(Command::new(id.clone(), "test".to_string(), None))
            .await
            .ok();

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_stream() {
        let _ = env_logger::try_init();
        let id = Id::new("test-stream", "test");

        let commands = Commands::new();

        let mut receiver = commands.subscribe(id.clone());

        let handle = tokio::spawn(async move {
            for i in 0..5 {
                let cmd = receiver.recv().await;
                log::info!("Received {:?}", cmd);
                assert_eq!(cmd.unwrap().command, format!("test{}", i).to_string());
            }
            let cmd2 = receiver.recv().await;
            log::info!("Received {:?}", cmd2);
            assert_eq!(cmd2.is_none(), true);
        });

        commands
            .send(Command::new(id.clone(), "test0".to_string(), None))
            .await
            .ok();
        commands
            .send(Command::new(id.clone(), "test1".to_string(), None))
            .await
            .ok();
        commands
            .send(Command::new(id.clone(), "test2".to_string(), None))
            .await
            .ok();
        commands
            .send(Command::new(id.clone(), "test3".to_string(), None))
            .await
            .ok();
        commands
            .send(Command::new(id.clone(), "test4".to_string(), None))
            .await
            .ok();

        commands.unsubscribe(id.clone());
        commands
            .send(Command::new(id.clone(), "test5".to_string(), None))
            .await
            .ok();

        handle.await.unwrap();
    }
}
