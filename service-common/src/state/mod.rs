mod config;
mod mux;

pub use self::config::*;
pub use mux::*;

use crate::client::DeviceStateClient;
use chrono::{DateTime, Utc};
use drogue_client::{
    error::{ClientError, ErrorInformation},
    registry,
};
use drogue_cloud_service_api::services::device_state::{
    self, DeleteOptions, DeviceState, Id, InitResponse, LastWillTestament,
};
use futures::{channel::mpsc::UnboundedReceiver, stream::FusedStream};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::Mutex,
    time::{sleep, Instant},
};
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct CreateOptions {
    pub lwt: Option<LastWillTestament>,
}

#[derive(Clone, Debug)]
pub struct StateController {
    mux: Arc<Mutex<Mux>>,
    client: DeviceStateClient,
    session: String,
    endpoint: String,
    retry_deletes: usize,
}

pub struct StateRunner {
    client: DeviceStateClient,
    session: String,
    mux: Arc<Mutex<Mux>>,
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

pub enum CreationOutcome {
    Created(Box<State>),
    Occupied,
    Failed,
}

impl StateController {
    pub async fn new(config: StateControllerConfiguration) -> anyhow::Result<(Self, StateRunner)> {
        let client = DeviceStateClient::from_config(config.client).await?;

        if let Some(init_delay) = config.init_delay {
            log::info!(
                "Delaying initialization by: {}",
                humantime::Duration::from(init_delay)
            );
            sleep(init_delay).await;
        }

        let InitResponse { session, expires } = {
            let mut attempts = config.retry_init;
            loop {
                match client.init().await {
                    Ok(response) => break response,
                    Err(err) => {
                        if attempts > 0 {
                            log::warn!(
                                "Failed to initialize. Will re-try {attempts} more times: {err}"
                            );
                            attempts -= 1;
                            sleep(Duration::from_secs(1)).await;
                        } else {
                            anyhow::bail!("Failed it create state service session.");
                        }
                    }
                }
            }
        };

        log::info!("Acquired new session: {session}");

        let mux = Arc::new(Mutex::new(Mux::new()));

        Ok((
            Self {
                mux: mux.clone(),
                client: client.clone(),
                session: session.clone(),
                endpoint: config.endpoint,
                retry_deletes: config.retry_deletes,
            },
            StateRunner {
                client,
                session,
                expires,
                mux,
                delay_buffer: chrono::Duration::from_std(config.delay_buffer)?,
                min_delay_ms: config.min_delay.as_millis() as _,
            },
        ))
    }

    pub async fn create(
        &self,
        application: &registry::v1::Application,
        device: &registry::v1::Device,
        max_attempts: usize,
        opts: CreateOptions,
    ) -> CreationOutcome {
        let state = DeviceState {
            device_uid: device.metadata.uid.clone(),
            endpoint: self.endpoint.clone(),
            lwt: opts.lwt,
        };

        let token = Uuid::new_v4().to_string();
        let mut attempts = max_attempts;

        loop {
            match self
                .client
                .create(
                    &self.session,
                    &application.metadata.name,
                    &device.metadata.name,
                    &token,
                    state.clone(),
                )
                .await
            {
                Ok(device_state::CreateResponse::Created) => {
                    let id = Id {
                        application: application.metadata.name.to_string(),
                        device: device.metadata.name.to_string(),
                    };
                    return CreationOutcome::Created(Box::new(State {
                        handle: StateHandle {
                            mux: self.mux.clone(),
                            deleted: false,
                            application: application.metadata.name.clone(),
                            device: device.metadata.name.clone(),
                            token: token.clone(),
                            state: self.clone(),
                        },
                        watcher: self.mux.lock().await.added(id, token),
                    }));
                }
                Ok(device_state::CreateResponse::Occupied) => {
                    if attempts > 0 {
                        log::debug!(
                            "Device state is still occupied (attempts left: {})",
                            attempts
                        );

                        attempts -= 1;

                        sleep(Duration::from_secs(1)).await;
                    } else {
                        log::info!(
                            "Device state still occupied after {} attempts",
                            max_attempts
                        );
                        return CreationOutcome::Occupied;
                    }
                }
                Err(err) => {
                    // we cannot be sure if the state was created or not. So we try to delete
                    // with our token. If that call is successful, it will clean up the mess.
                    // If that call isn't successful, then we panic, and the (pod) session timeout
                    // will clean up for us.
                    log::info!("Failed to create state: {err}. Trying to recover...");
                    self.delete(
                        &application.metadata.name,
                        &device.metadata.name,
                        &token,
                        Default::default(),
                    )
                    .await;
                    return CreationOutcome::Failed;
                }
            }
        }
    }

    /// Delete device state.
    ///
    /// This function will **panic** in the case that state service cannot be contacted, even after re-trying.
    pub async fn delete(&self, application: &str, device: &str, token: &str, opts: DeleteOptions) {
        let mut attempts = self.retry_deletes;
        let delay = Duration::from_millis(250);

        loop {
            match self
                .client
                .delete(&self.session, application, device, token, &opts)
                .await
            {
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
                Err(ClientError::Service {
                    error: ErrorInformation { error, .. },
                    ..
                }) if error == "NotInitialized" => {
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
        for id in lost_ids {
            self.mux.lock().await.handle_lost(id).await;
        }

        Ok(())
    }
}
