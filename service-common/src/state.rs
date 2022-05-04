use crate::client::{DeviceStateClient, DeviceStateClientConfig};
use anyhow::Context;
use chrono::{DateTime, Utc};
use drogue_client::{
    error::{ClientError, ErrorInformation},
    registry,
};
use drogue_cloud_service_api::services::device_state::{
    CreateResponse, DeviceState, Id, InitResponse,
};
use futures::{
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    stream::FusedStream,
    SinkExt,
};
use serde::Deserialize;
use std::{
    ops::{Deref, DerefMut},
    time::Duration,
};
use tokio::time::{sleep, Instant};

#[derive(Clone, Debug, Deserialize)]
pub struct StateControllerConfiguration {
    #[serde(default)]
    pub client: DeviceStateClientConfig,
    pub endpoint: String,
    /// The amount of time to ping again before the session expiration.
    #[serde(with = "humantime_serde", default = "default_delay_buffer")]
    pub delay_buffer: Duration,
    /// The minimum delay time to wait before another ping.
    #[serde(with = "humantime_serde", default = "default_min_delay")]
    pub min_delay: Duration,
}

impl Default for StateControllerConfiguration {
    fn default() -> Self {
        Self {
            client: Default::default(),
            endpoint: "default".to_string(),
            delay_buffer: default_delay_buffer(),
            min_delay: default_min_delay(),
        }
    }
}

const fn default_delay_buffer() -> Duration {
    Duration::from_secs(5)
}

const fn default_min_delay() -> Duration {
    Duration::from_secs(1)
}

#[derive(Clone, Debug)]
pub struct StateController {
    client: DeviceStateClient,
    session: String,
    endpoint: String,
}

pub struct StateRunner {
    client: DeviceStateClient,
    session: String,
    sender: UnboundedSender<Id>,
    expires: DateTime<Utc>,
    delay_buffer: chrono::Duration,
    min_delay_ms: i64,
}

pub struct StateStream {
    stream: UnboundedReceiver<Id>,
}

impl Deref for StateStream {
    type Target = dyn FusedStream<Item = Id>;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl DerefMut for StateStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}

impl StateController {
    pub async fn new(
        config: StateControllerConfiguration,
    ) -> anyhow::Result<(Self, StateRunner, StateStream)> {
        let client = DeviceStateClient::from_config(config.client).await?;

        let InitResponse { session, expires } = client.init().await?;

        let (tx, rx) = futures::channel::mpsc::unbounded::<Id>();

        Ok((
            Self {
                client: client.clone(),
                session: session.clone(),
                endpoint: config.endpoint,
            },
            StateRunner {
                client,
                session,
                expires,
                sender: tx,
                delay_buffer: chrono::Duration::from_std(config.delay_buffer)?,
                min_delay_ms: config.min_delay.as_millis() as _,
            },
            StateStream { stream: rx },
        ))
    }

    pub async fn create(
        &self,
        application: &registry::v1::Application,
        device: &registry::v1::Device,
    ) -> Result<CreateResponse, ClientError> {
        let state = DeviceState {
            device_uid: device.metadata.uid.clone(),
            endpoint: self.endpoint.clone(),
        };

        self.client
            .create(
                &self.session,
                &application.metadata.name,
                &device.metadata.name,
                state,
            )
            .await
    }

    /// Delete device state.
    ///
    /// This function might **panic** in case the state service cannot be contacted, even re-trying.
    pub async fn delete(&self, application: &str, device: &str) {
        let mut attempts = 10;
        let delay = Duration::from_millis(250);

        loop {
            match self.client.delete(&self.session, application, device).await {
                Ok(_) => break,
                Err(err) => {
                    attempts -= 1;
                    log::error!("Failed to communicate with state service (attempts left: {attempts}): {err}");
                    if attempts == 0 {
                        panic!("Unable to contact state service. Last error was: {err}");
                    } else {
                        sleep(delay).await;
                    }
                }
            }
        }
    }
}

impl StateRunner {
    pub async fn run(self) -> anyhow::Result<()> {
        // the first deadline
        let (mut expires, mut deadline) = self.next_deadline(self.expires);
        loop {
            tokio::time::sleep_until(deadline).await;
            (expires, deadline) = self.ping(expires).await?;
        }
    }

    fn next_deadline(&self, expires: DateTime<Utc>) -> (DateTime<Utc>, Instant) {
        // expiration, minus the buffer
        let deadline = expires - self.delay_buffer;
        // delay from now in milliseconds
        let delay = (deadline - Utc::now()).num_milliseconds();
        // ensure the minimum
        let delay = if delay < self.min_delay_ms {
            self.min_delay_ms as u64
        } else {
            delay as u64
        };

        // convert
        let delay = Duration::from_millis(delay);
        let deadline = Instant::now() + delay;

        log::debug!(
            "Next expiration: {expires}, next ping: {} ms",
            delay.as_millis()
        );

        // result
        (expires, deadline)
    }

    pub async fn ping(&self, expires: DateTime<Utc>) -> anyhow::Result<(DateTime<Utc>, Instant)> {
        loop {
            if Utc::now() > expires {
                anyhow::bail!("Lost session. Must terminate.");
            }

            match self.client.ping(&self.session).await {
                Ok(response) => {
                    if !response.lost_ids.is_empty() {
                        self.handle_lost(response.lost_ids).await?;
                    }

                    return Ok(self.next_deadline(response.expires));
                }
                Err(ClientError::Service(ErrorInformation { error, .. }))
                    if error == "NotInitialized" =>
                {
                    // we lost the session
                    anyhow::bail!("Session got invalidated. Must terminate.");
                }
                Err(err) => {
                    log::warn!("Failed to ping: {err}");
                    sleep(Duration::from_millis(100)).await;
                    // continue trying
                }
            }
        }
    }

    pub async fn handle_lost(&self, lost_ids: Vec<Id>) -> anyhow::Result<()> {
        let mut sender = &self.sender;

        for id in lost_ids {
            sender.feed(id).await.context("Feeding lost ID to stream")?;
        }
        sender.flush().await.context("Flushing lost ID stream")?;

        Ok(())
    }
}
