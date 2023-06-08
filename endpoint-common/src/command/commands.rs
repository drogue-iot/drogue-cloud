use crate::command::{
    Command, CommandAddress, CommandDispatcher, CommandNameFilter, CommandTarget,
};
use async_trait::async_trait;
use drogue_cloud_service_common::Id;
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
    hash::Hash,
    sync::Arc,
};
use tokio::sync::{
    mpsc::{channel, Receiver},
    Mutex,
};

/// A filter for commands
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CommandFilter {
    pub application: String,
    pub gateway: String,
    pub device: Option<String>,
    pub command_filter: Option<String>,
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
            command_filter: None,
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
            command_filter: None,
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
            command_filter: None,
        }
    }

    /// Override the current command filter.
    ///
    /// The command filter is an MQTT topic filter applied to the command name.
    pub fn with_filter<T>(self, command_filter: T) -> Self
    where
        T: Into<Option<String>>,
    {
        Self {
            command_filter: command_filter.into(),
            ..self
        }
    }
}

type CommandMap<T> = Arc<Mutex<HashMap<T, HashMap<usize, CommandTarget>>>>;

/// Command dispatching implementation.
#[derive(Clone, Debug)]
pub struct Commands {
    devices: CommandMap<CommandAddress>,
    wildcards: CommandMap<Id>,
}

impl Default for Commands {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct SubscriptionHandle {
    filter: CommandFilter,
    id: usize,
}

#[derive(Debug)]
pub struct Subscription {
    pub receiver: Receiver<Command>,
    pub handle: SubscriptionHandle,
}

impl Commands {
    pub fn new() -> Self {
        log::info!("New internal command broker");
        Self {
            devices: Arc::new(Mutex::new(HashMap::new())),
            wildcards: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn subscribe(&self, filter: CommandFilter) -> Subscription {
        // FIXME: must need to handle multiple subscriptions to the same filter
        log::debug!("Subscribe {:?} to receive commands", filter);

        let (tx, rx) = channel(32);

        // create a filter, if that would fail, we silently ignore it and never match.
        let command_filter = CommandNameFilter::from(&filter.command_filter);

        // if the device is set, and equal to the gateway, it is the same
        let filter = match filter.device {
            Some(device) if device == filter.gateway => {
                let filter = CommandFilter {
                    device: None,
                    ..filter
                };
                log::debug!("Cleanup filter to: {filter:?}");
                filter
            }
            _ => filter,
        };

        let id = match filter.device.clone() {
            Some(device) => {
                let mut devices = self.devices.lock().await;
                let address =
                    CommandAddress::new(filter.application.clone(), filter.gateway.clone(), device);

                Self::add_entry(
                    &mut devices,
                    address,
                    CommandTarget {
                        tx,
                        filter: command_filter,
                    },
                )
            }
            None => {
                let mut gateways = self.wildcards.lock().await;
                let id = Id::new(filter.application.clone(), filter.gateway.clone());

                Self::add_entry(
                    &mut gateways,
                    id,
                    CommandTarget {
                        tx,
                        filter: command_filter,
                    },
                )
            }
        };

        Subscription {
            receiver: rx,
            handle: SubscriptionHandle { id, filter },
        }
    }

    pub async fn unsubscribe<SH>(&self, subscription: SH)
    where
        SH: Into<SubscriptionHandle>,
    {
        let handle = subscription.into();

        log::debug!("Unsubscribe {:?} from receiving commands", handle);

        // TODO: try to remove cloning all values

        match &handle.filter.device {
            Some(device) => {
                let mut devices = self.devices.lock().await;
                Self::remove_entry(
                    &mut devices,
                    CommandAddress::new(
                        handle.filter.application.clone(),
                        handle.filter.gateway.clone(),
                        device,
                    ),
                    handle.id,
                );
            }
            None => {
                let mut gateways = self.wildcards.lock().await;
                Self::remove_entry(
                    &mut gateways,
                    Id::new(
                        handle.filter.application.clone(),
                        handle.filter.gateway.clone(),
                    ),
                    handle.id,
                );
            }
        }
    }

    fn add_entry<K, V>(map: &mut HashMap<K, HashMap<usize, V>>, key: K, value: V) -> usize
    where
        K: Eq + Hash + Debug,
    {
        log::debug!("Adding entry for: {key:?}");

        let entry_map = match map.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(HashMap::new()),
        };

        loop {
            let id: usize = rand::random();

            match entry_map.entry(id) {
                Entry::Vacant(entry) => {
                    // entry was free, we can insert
                    entry.insert(value);
                    break id;
                }
                Entry::Occupied(_) => {
                    log::debug!("ID clash, retrying");
                    // entry is occupied, we need to re-try
                }
            }
        }
    }

    fn remove_entry<K, V>(map: &mut HashMap<K, HashMap<usize, V>>, key: K, id: usize)
    where
        K: Eq + Hash + Debug,
    {
        match map.entry(key) {
            Entry::Vacant(_) => {}
            Entry::Occupied(mut entry) => {
                let map = entry.get_mut();
                map.remove(&id);
                if map.is_empty() {
                    entry.remove();
                }
            }
        }
    }
}

#[async_trait]
impl CommandDispatcher for Commands {
    async fn send(&self, msg: Command) {
        // TODO: try to reduce cloning

        log::debug!("Dispatching command to {:?}", msg.address);

        let mut possible: usize = 0;
        let mut num: usize = 0;

        if let Some(senders) = self.devices.lock().await.get(&msg.address) {
            possible += senders.len();
            log::debug!(
                "Sending command {:?} sent to device {:?}",
                msg.command,
                msg.address
            );
            num += dispatch_command(senders.values(), &msg).await;
        }

        if let Some(senders) = self.wildcards.lock().await.get(&Id::new(
            msg.address.app_id.clone(),
            msg.address.gateway_id.clone(),
        )) {
            possible += senders.len();
            log::debug!(
                "Sending command {:?} sent to wildcard {:?}",
                msg.command,
                msg.address
            );
            num += dispatch_command(senders.values(), &msg).await;
        }

        log::debug!("Sent to {num} receivers of {possible}");
    }
}

/// Dispatch a command to a list of senders/devices.
async fn dispatch_command<'a, I>(senders: I, msg: &Command) -> usize
where
    I: IntoIterator<Item = &'a CommandTarget>,
{
    let mut num = 0;

    for sender in senders {
        if !sender.filter.matches(&msg.command) {
            continue;
        }

        num += 1;
        match sender.tx.send(msg.clone()).await {
            Ok(_) => {
                log::debug!("Command sent");
            }
            Err(e) => {
                log::error!("Failed to send a command {:?}", e);
            }
        }
    }

    num
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

        let Subscription { mut receiver, .. } = commands.subscribe(filter).await;

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

        let Subscription {
            mut receiver,
            handle: sh,
        } = commands.subscribe(filter.clone()).await;

        // handle

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

        commands.unsubscribe(sh).await;

        // the following commands must not be received

        commands
            .send(Command::new(address.clone(), "test5".to_string(), None))
            .await;

        // await

        handle.await.unwrap();
    }

    #[derive(Default)]
    struct MockReceiverResult {
        finished: bool,
        commands: Vec<Command>,
    }

    fn mock_receiver(
        receiver: Subscription,
    ) -> (
        Arc<Mutex<MockReceiverResult>>,
        SubscriptionHandle,
        JoinHandle<()>,
    ) {
        let Subscription {
            mut receiver,
            handle: sh,
        } = receiver;

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

        (r, sh, handle)
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

    fn cmds<G, D, C, I>(gateway: G, device: D, commands: I) -> Vec<Command>
    where
        G: Into<String>,
        D: Into<String>,
        C: Into<String>,
        I: IntoIterator<Item = C>,
    {
        let gateway = gateway.into();
        let device = device.into();

        commands
            .into_iter()
            .map(|command| {
                Command::new(
                    CommandAddress::new(APP, gateway.clone(), device.clone()),
                    command.into(),
                    None,
                )
            })
            .collect()
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
            let (d1, _, handle) = mock_receiver(commands.subscribe(d1).await);
            handles.push(handle);

            // a simple gateway device
            let (gw1_all, _, handle) = mock_receiver(commands.subscribe(gw1_all).await);
            handles.push(handle);

            // a duplicate
            let (gw1_all_2, _, handle) = mock_receiver(commands.subscribe(gw1_all_2).await);
            handles.push(handle);

            // a gateway device for a specific device
            let (gw1_d1, _, handle) = mock_receiver(commands.subscribe(gw1_d1).await);
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

    #[tokio::test]
    async fn test_command_filters() {
        let _ = env_logger::try_init();

        let (handles, d1f1, d1f2, d1f3, d1f4) = {
            let commands = Commands::new();

            let d1f1 = CommandFilter::device(APP, "d1").with_filter("#".to_string());
            let d1f2 = CommandFilter::device(APP, "d1").with_filter("foo".to_string());
            let d1f3 = CommandFilter::device(APP, "d1").with_filter("bar/+/baz".to_string());
            let d1f4 = CommandFilter::device(APP, "d1").with_filter("bar/baz/#".to_string());

            let mut handles = vec![];

            let (d1f1, _, handle) = mock_receiver(commands.subscribe(d1f1).await);
            handles.push(handle);
            let (d1f2, _, handle) = mock_receiver(commands.subscribe(d1f2).await);
            handles.push(handle);
            let (d1f3, _, handle) = mock_receiver(commands.subscribe(d1f3).await);
            handles.push(handle);
            let (d1f4, _, handle) = mock_receiver(commands.subscribe(d1f4).await);
            handles.push(handle);

            // send commands

            commands.send(cmd("d1", "d1", "foo-bar-baz")).await;

            commands.send(cmd("d1", "d1", "foo")).await;

            commands.send(cmd("d1", "d1", "bar/abc/baz")).await;

            commands.send(cmd("d1", "d1", "bar/baz/a")).await;
            commands.send(cmd("d1", "d1", "bar/baz/a/b/c")).await;

            // return

            (handles, d1f1, d1f2, d1f3, d1f4)
        };

        // wait for all receivers to complete

        select_all(handles).await.0.unwrap();

        // access data

        let d1f1 = d1f1.lock().await;
        let d1f2 = d1f2.lock().await;
        let d1f3 = d1f3.lock().await;
        let d1f4 = d1f4.lock().await;

        assert!(d1f1.finished);
        assert_eq!(
            d1f1.commands,
            cmds(
                "d1",
                "d1",
                [
                    "foo-bar-baz",
                    "foo",
                    "bar/abc/baz",
                    "bar/baz/a",
                    "bar/baz/a/b/c"
                ]
            ),
            "d1f1 outcome"
        );

        assert!(d1f2.finished);
        assert_eq!(d1f2.commands, cmds("d1", "d1", ["foo"]), "d1f2 outcome");

        assert!(d1f3.finished);
        assert_eq!(
            d1f3.commands,
            cmds("d1", "d1", ["bar/abc/baz",]),
            "d1f3 outcome"
        );

        assert!(d1f4.finished);
        assert_eq!(
            d1f4.commands,
            cmds("d1", "d1", ["bar/baz/a", "bar/baz/a/b/c"]),
            "d1f4 outcome"
        );
    }
}
