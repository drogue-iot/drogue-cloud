use crate::command::{Command, CommandDispatcher};
use async_std::sync::Mutex;
use async_trait::async_trait;
use drogue_cloud_service_common::Id;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{channel, Receiver, Sender};

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

    pub async fn subscribe(&self, device_id: Id) -> Receiver<Command> {
        let (tx, rx) = channel(32);
        let mut devices = self.devices.lock().await;
        log::debug!("Device {:?} subscribed to receive commands", device_id);
        devices.insert(device_id, tx);
        rx
    }

    pub async fn unsubscribe(&self, device_id: &Id) {
        let mut devices = self.devices.lock().await;
        devices.remove(device_id);
        log::debug!(
            "Device {:?} unsubscribed from receiving commands",
            device_id
        );
    }
}

#[async_trait]
impl CommandDispatcher for Commands {
    async fn send(&self, msg: Command) -> Result<(), String> {
        if let Some(sender) = self.devices.lock().await.get(&msg.device_id) {
            log::debug!(
                "Sending command {:?} sent to device {:?}",
                msg.command.clone(),
                msg.device_id
            );
            match sender.send(msg).await {
                Ok(_) => {
                    log::debug!("Command sent");
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

        let mut receiver = commands.subscribe(id.clone()).await;

        let handle = tokio::spawn(async move {
            let cmd = timeout(Duration::from_secs(1), receiver.recv()).await;
            log::info!("Received {:?}", cmd);
            assert_eq!(cmd.unwrap().unwrap().command, "test".to_string());
            let cmd2 = timeout(Duration::from_secs(1), receiver.recv()).await;
            log::info!("Received {:?}", cmd2);
            assert!(cmd2.is_err());
        });

        commands
            .send(Command::new(id, "test".to_string(), None))
            .await
            .ok();

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_stream() {
        let _ = env_logger::try_init();
        let id = Id::new("test-stream", "test");

        let commands = Commands::new();

        let mut receiver = commands.subscribe(id.clone()).await;

        let handle = tokio::spawn(async move {
            for i in 0..5 {
                let cmd = receiver.recv().await;
                log::info!("Received {:?}", cmd);
                assert_eq!(cmd.unwrap().command, format!("test{}", i).to_string());
            }
            let cmd2 = receiver.recv().await;
            log::info!("Received {:?}", cmd2);
            assert!(cmd2.is_none());
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

        commands.unsubscribe(&id).await;
        commands
            .send(Command::new(id.clone(), "test5".to_string(), None))
            .await
            .ok();

        handle.await.unwrap();
    }
}
