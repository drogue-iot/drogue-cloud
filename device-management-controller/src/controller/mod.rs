pub mod resource;

use crate::controller::resource::{ApplicationAndDevice, ApplicationAndDeviceKey};
use async_trait::async_trait;
use drogue_client::registry::{self};
use drogue_cloud_endpoint_common::sender::{
    DownstreamSender, Publish, PublishId, PublishOptions, PublishOutcome, Publisher,
};
use drogue_cloud_operator_common::controller::{
    base::{ControllerOperation, ProcessOutcome},
    reconciler::ReconcileError,
};
use serde::Deserialize;
use std::{ops::Deref, time::Duration};

pub const REGISTRY_TYPE_EVENT: &str = "io.drogue.registry.v1";

#[derive(Clone, Debug, Deserialize)]
pub struct ControllerConfig {
    #[serde(default = "default_retry_full")]
    #[serde(with = "humantime_serde")]
    pub retry_full: Duration,

    #[serde(default = "default_retry_failed")]
    #[serde(with = "humantime_serde")]
    pub retry_failed: Duration,
}

const fn default_retry_full() -> Duration {
    Duration::from_secs(1)
}
const fn default_retry_failed() -> Duration {
    Duration::from_secs(10)
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            retry_full: default_retry_full(),
            retry_failed: default_retry_failed(),
        }
    }
}

pub struct EventController {
    config: ControllerConfig,
    registry: registry::v1::Client,
    sender: DownstreamSender,
}

impl EventController {
    pub fn new(
        config: ControllerConfig,
        registry: registry::v1::Client,
        sender: DownstreamSender,
    ) -> Self {
        Self {
            config,
            registry,
            sender,
        }
    }
}

impl Deref for EventController {
    type Target = registry::v1::Client;

    fn deref(&self) -> &Self::Target {
        &self.registry
    }
}

#[async_trait]
impl ControllerOperation<ApplicationAndDeviceKey, ApplicationAndDevice, ()> for EventController {
    async fn process_resource(
        &self,
        resource: ApplicationAndDevice,
    ) -> Result<ProcessOutcome<()>, ReconcileError> {
        Ok(self.send_event(resource).await)
    }

    async fn recover(&self, _message: &str, _resource: ApplicationAndDevice) -> Result<(), ()> {
        Ok(())
    }
}

impl EventController {
    async fn send_event(&self, resource: ApplicationAndDevice) -> ProcessOutcome<()> {
        let publish = Publish {
            application: &resource.application,
            device: PublishId {
                name: resource.key.device.clone(),
                uid: Some(resource.key.device_uid.clone()),
            },
            sender: PublishId {
                name: resource.key.device,
                uid: Some(resource.key.device_uid),
            },
            channel: "devices".into(),
            options: PublishOptions {
                r#type: Some(REGISTRY_TYPE_EVENT.into()),
                ..Default::default()
            },
        };

        let outcome = self.sender.publish(publish, vec![]).await;

        log::debug!("Publish outcome: {outcome:?}");

        match outcome {
            Ok(PublishOutcome::Accepted) => ProcessOutcome::Complete(()),
            Ok(PublishOutcome::QueueFull) => {
                ProcessOutcome::Retry((), Some(self.config.retry_full))
            }
            Ok(PublishOutcome::Rejected) => {
                // we will retry until we find out that the application is gone.
                ProcessOutcome::Retry((), Some(self.config.retry_failed))
            }
            Err(err) => {
                log::warn!("Failed to start publishing the event. Skipping: {err}");
                ProcessOutcome::Complete(())
            }
        }
    }
}
