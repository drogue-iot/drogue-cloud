use crate::{
    command::{Command, CommandDispatcher},
    kafka::KafkaConfig,
};
use async_trait::async_trait;
use drogue_cloud_event_common::stream::{
    AutoAck, EventStream, EventStreamConfig, EventStreamError,
};
use drogue_cloud_service_api::health::{HealthCheckError, HealthChecked};
use drogue_cloud_service_common::defaults;
use futures::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use std::{
    convert::TryFrom,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::task::JoinHandle;

#[derive(Clone, Debug, Deserialize)]
pub struct KafkaCommandSourceConfig {
    #[serde(default, flatten)]
    pub kafka: KafkaConfig,
    #[serde(default = "defaults::kafka_command_topic")]
    pub topic: String,
    pub consumer_group: String,
}

pub struct KafkaCommandSource {
    handle: JoinHandle<()>,
    alive: Arc<AtomicBool>,
}

impl KafkaCommandSource {
    pub fn new<D>(dispatcher: D, config: KafkaCommandSourceConfig) -> Result<Self, EventStreamError>
    where
        D: CommandDispatcher + Send + Sync + 'static,
    {
        let mut source = EventStream::<AutoAck>::new(EventStreamConfig {
            bootstrap_servers: config.kafka.bootstrap_servers,
            properties: config.kafka.custom,
            topic: config.topic,
            consumer_group: Some(config.consumer_group),
        })?;

        let alive = Arc::new(AtomicBool::new(true));
        let a = alive.clone();

        let handle = tokio::spawn(async move {
            while let Some(event) = source.next().await {
                match event {
                    Ok(event) => match Command::try_from(event) {
                        Ok(command) => {
                            if let Err(err) = dispatcher.send(command).await {
                                log::info!("Failed to dispatch command: {}", err);
                            }
                        }
                        Err(_) => {
                            log::info!("Failed to convert event to command");
                        }
                    },
                    Err(err) => {
                        log::info!("Failed to read next event: {}", err);
                    }
                }
            }
            log::info!("Exiting event loop!");
            a.store(false, Ordering::Relaxed);
        });

        Ok(Self { handle, alive })
    }
}

impl Drop for KafkaCommandSource {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[async_trait]
impl HealthChecked for KafkaCommandSource {
    async fn is_alive(&self) -> Result<(), HealthCheckError> {
        if self.alive.load(Ordering::Relaxed) {
            Ok(())
        } else {
            HealthCheckError::nok("Event loop is not alive")
        }
    }
}
