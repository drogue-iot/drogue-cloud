use crate::command::{Command, CommandAddress, CommandDispatcher};
use async_std::sync::Mutex;
use async_trait::async_trait;
use drogue_cloud_service_common::Id;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{channel, Receiver, Sender};

/// A filter for commands
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CommandFilter {
    pub application: String,
    pub gateway: String,
    pub device: Option<String>,
}

impl CommandFilter {
    /// A filter for commands for all devices attached to the gateway
    pub fn wildcard<A, G>(application: A, gateway: G) -> Self
    where
        A: Into<String>,
        G: Into<String>,
    {
        Self {
            application: application.into(),
            gateway: gateway.into(),
            device: None,
        }
    }

    /// A filter for commands for a specific device attached to a gateway
    pub fn proxied_device<A, G, D>(application: A, gateway: G, device: D) -> Self
    where
        A: Into<String>,
        G: Into<String>,
        D: Into<String>,
    {
        Self {
            application: application.into(),
            gateway: gateway.into(),
            device: Some(device.into()),
        }
    }

    /// A filter for a specific device
    pub fn device<A, D>(application: A, device: D) -> Self
    where
        A: Into<String>,
        D: Into<String>,
    {
        let device = device.into();
        Self {
            application: application.into(),
            gateway: device.clone(),
            device: Some(device),
        }
    }
}

/// Command dispatching implementation.
#[derive(Clone, Debug)]
pub struct Commands {
    devices: Arc<Mutex<HashMap<CommandAddress, Sender<Command>>>>,
    wildcards: Arc<Mutex<HashMap<Id, Sender<Command>>>>,
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
            wildcards: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn subscribe(&self, filter: CommandFilter) -> Receiver<Command> {
        // FIXME: must need to handle multiple subscriptions to the same filter
        log::debug!("Subscribe {:?} to receive commands", filter);

        let (tx, rx) = channel(32);

        match filter.device {
            Some(device) => {
                let mut devices = self.devices.lock().await;
                devices.insert(
                    CommandAddress::new(filter.application, filter.gateway, device),
                    tx,
                );
            }
            None => {
                let mut gateways = self.wildcards.lock().await;
                gateways.insert(Id::new(filter.application, filter.gateway), tx);
            }
        }

        rx
    }

    pub async fn unsubscribe(&self, filter: &CommandFilter) {
        log::debug!("Unsubscribe {:?} from receiving commands", filter);

        // TODO: try to remove cloning all values

        match &filter.device {
            Some(device) => {
                let mut devices = self.devices.lock().await;
                devices.remove(&CommandAddress::new(
                    filter.application.clone(),
                    filter.gateway.clone(),
                    device,
                ));
            }
            None => {
                let mut gateways = self.wildcards.lock().await;
                gateways.remove(&Id::new(filter.application.clone(), filter.gateway.clone()));
            }
        }
    }
}

#[async_trait]
impl CommandDispatcher for Commands {
    async fn send(&self, msg: Command) {
        // TODO: try to reduce cloning

        log::debug!("Dispatching command to {:?}", msg.address);

        let mut num: usize = 0;

        if let Some(sender) = self.devices.lock().await.get(&msg.address) {
            num += 1;
            log::debug!(
                "Sending command {:?} sent to device {:?}",
                msg.command,
                msg.address
            );
            match sender.send(msg.clone()).await {
                Ok(_) => {
                    log::debug!("Command sent");
                }
                Err(e) => {
                    log::error!("Failed to send a command {:?}", e);
                }
            }
        }

        if let Some(sender) = self.wildcards.lock().await.get(&Id::new(
            msg.address.app_id.clone(),
            msg.address.gateway_id.clone(),
        )) {
            num += 1;
            log::debug!(
                "Sending command {:?} sent to wildcard {:?}",
                msg.command,
                msg.address
            );
            match sender.send(msg).await {
                Ok(_) => {
                    log::debug!("Command sent");
                }
                Err(e) => {
                    log::error!("Failed to send a command {:?}", e);
                }
            }
        }

        log::debug!("Sent to {} receivers", num);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::command::CommandAddress;
    use futures::future::select_all;
    use tokio::{
        task::JoinHandle,
        time::{timeout, Duration},
    };

    const APP: &str = "test-app";

    #[tokio::test]
    async fn test_timeout() {
        let _ = env_logger::try_init();
        let filter = CommandFilter::device("test-timeout", "test");

        let commands = Commands::new();

        let mut receiver = commands.subscribe(filter).await;

        let handle = tokio::spawn(async move {
            let cmd = timeout(Duration::from_secs(1), receiver.recv()).await;
            log::info!("Received {:?}", cmd);
            assert_eq!(cmd.unwrap().unwrap().command, "test".to_string());
            let cmd2 = timeout(Duration::from_secs(1), receiver.recv()).await;
            log::info!("Received {:?}", cmd2);
            assert!(cmd2.is_err());
        });

        let address = CommandAddress::new("test-timeout", "test", "test");
        commands
            .send(Command::new(address, "test".to_string(), None))
            .await;

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_stream() {
        let _ = env_logger::try_init();

        let address = CommandAddress::new("test-stream", "test", "test");
        let filter = CommandFilter::device("test-stream", "test");

        let commands = Commands::new();

        let mut receiver = commands.subscribe(filter.clone()).await;

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

        // send commands

        commands
            .send(Command::new(address.clone(), "test0".to_string(), None))
            .await;
        commands
            .send(Command::new(address.clone(), "test1".to_string(), None))
            .await;
        commands
            .send(Command::new(address.clone(), "test2".to_string(), None))
            .await;
        commands
            .send(Command::new(address.clone(), "test3".to_string(), None))
            .await;
        commands
            .send(Command::new(address.clone(), "test4".to_string(), None))
            .await;

        // unsubscribe

        commands.unsubscribe(&filter).await;

        // the following commands must not be received

        commands
            .send(Command::new(address.clone(), "test5".to_string(), None))
            .await;

        handle.await.unwrap();
    }

    #[derive(Default)]
    struct MockReceiverResult {
        finished: bool,
        commands: Vec<Command>,
    }

    fn mock_receiver(
        mut receiver: Receiver<Command>,
    ) -> (Arc<Mutex<MockReceiverResult>>, JoinHandle<()>) {
        let r = Arc::new(Mutex::new(MockReceiverResult::default()));

        let inner_r = r.clone();
        let handle = tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    None => {
                        inner_r.lock().await.finished = true;
                        break;
                    }
                    Some(command) => {
                        inner_r.lock().await.commands.push(command);
                    }
                }
            }
        });

        (r, handle)
    }

    fn cmd<G, D, C>(gateway: G, device: D, command: C) -> Command
    where
        G: Into<String>,
        D: Into<String>,
        C: Into<String>,
    {
        Command::new(
            CommandAddress::new(APP, gateway, device),
            command.into(),
            None,
        )
    }

    #[tokio::test]
    async fn test_filters() {
        let _ = env_logger::try_init();

        let (handles, d1, gw1_all, gw1_all_2, gw1_d1) = {
            let commands = Commands::new();

            let d1 = CommandFilter::device(APP, "d1");
            let gw1_all = CommandFilter::wildcard(APP, "gw1");
            let gw1_all_2 = gw1_all.clone();
            let gw1_d1 = CommandFilter::proxied_device(APP, "gw1", "d1");

            let mut handles = vec![];

            // a simple device
            let (d1, handle) = mock_receiver(commands.subscribe(d1).await);
            handles.push(handle);

            // a simple gateway device
            let (gw1_all, handle) = mock_receiver(commands.subscribe(gw1_all).await);
            handles.push(handle);

            // a duplicate
            let (gw1_all_2, handle) = mock_receiver(commands.subscribe(gw1_all_2).await);
            handles.push(handle);

            // a gateway device for a specific device
            let (gw1_d1, handle) = mock_receiver(commands.subscribe(gw1_d1).await);
            handles.push(handle);

            // send commands

            commands.send(cmd("d1", "d1", "d1-d1")).await;
            commands.send(cmd("gw1", "d1", "gw1-d1")).await;
            commands.send(cmd("gw1", "d2", "gw1-d2")).await;
            commands.send(cmd("d2", "d2", "d1-d1")).await;

            // return

            (handles, d1, gw1_all, gw1_all_2, gw1_d1)
        };

        // wait for all receivers to complete

        select_all(handles).await.0.unwrap();

        // access data

        let d1 = d1.lock().await;
        let gw1_all = gw1_all.lock().await;
        let gw1_all_2 = gw1_all_2.lock().await;
        let gw1_d1 = gw1_d1.lock().await;

        // assert

        assert!(d1.finished);
        assert_eq!(d1.commands, vec![cmd("d1", "d1", "d1-d1")], "d1 outcome");

        assert!(gw1_all.finished);
        assert_eq!(
            gw1_all.commands,
            vec![cmd("gw1", "d1", "gw1-d1"), cmd("gw1", "d2", "gw1-d2")],
            "gw1_all outcome"
        );

        assert!(gw1_all_2.finished);
        assert_eq!(
            gw1_all_2.commands,
            vec![cmd("gw1", "d1", "gw1-d1"), cmd("gw1", "d2", "gw1-d2")],
            "gw1_all_2 outcome"
        );

        assert!(gw1_d1.finished);
        assert_eq!(
            gw1_d1.commands,
            vec![cmd("gw1", "d1", "gw1-d1")],
            "gw1_d1 outcome"
        );
    }
}
