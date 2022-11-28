mod config;
mod mux;

pub use self::config::*;
pub use mux::*;

use crate::client::CommandRoutingClient;
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use drogue_client::{
    error::{ClientError, ErrorInformation},
    registry,
};
use drogue_cloud_service_api::{services::{command_routing::{
    self, CommandRoute, Id, InitResponse, LastWillTestament,
}}};
use futures::{channel::mpsc::UnboundedReceiver, stream::FusedStream};
use std::{
    ops::{Deref, DerefMut},
    process::abort,
    sync::Arc,
    time::Duration,
};
use tokio::{
    select,
    sync::{oneshot, Mutex},
    time::{sleep, Instant},
};
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct CreateOptions {
    pub lwt: Option<LastWillTestament>,
}

#[derive(Clone, Debug)]
pub struct CommandRoutingController {
    mux: Arc<Mutex<Mux>>,
    client: CommandRoutingClient,
    session: String,
    endpoint: String,
    retry_deletes: usize,
    kill_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

pub struct CommandRoutingRunner {
    client: CommandRoutingClient,
    session: String,
    mux: Arc<Mutex<Mux>>,
    expires: DateTime<Utc>,
    delay_buffer: chrono::Duration,
    min_delay_ms: i64,
    kill_rx: Option<oneshot::Receiver<()>>,
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

impl CommandRoutingController {
    pub async fn new(config: CommandRoutingControllerConfiguration) -> anyhow::Result<(Self, CommandRoutingRunner)> {
        let client = CommandRoutingClient::from_config(config.client).await?;

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
        let (kill_tx, kill_rx) = oneshot::channel();

        Ok((
            Self {
                mux: mux.clone(),
                client: client.clone(),
                session: session.clone(),
                endpoint: config.endpoint,
                retry_deletes: config.retry_deletes,
                kill_tx: Arc::new(Mutex::new(Some(kill_tx))),
            },
            CommandRoutingRunner {
                client,
                session,
                expires,
                mux,
                delay_buffer: chrono::Duration::from_std(config.delay_buffer)?,
                min_delay_ms: config.min_delay.as_millis() as _,
                kill_rx: Some(kill_rx),
            },
        ))
    }

    pub async fn create(
        &self,
        application: &registry::v1::Application,
        device: &registry::v1::Device,
        max_attempts: usize,
    ) -> CreationOutcome {
        let state = CommandRoute {
            device_uid: device.metadata.uid.clone(),
            endpoint: self.endpoint.clone(),
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
                Ok(command_routing::CreateResponse::Created) => {
                    let id = Id {
                        application: application.metadata.name.to_string(),
                        device: device.metadata.name.to_string(),
                    };
                    return CreationOutcome::Created(Box::new(State {
                        handle: RouteHandle {
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
                Ok(command_routing::CreateResponse::Occupied) => {
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
                    println!("Failed to create state: {err}. Trying to recover...");
                    log::info!("Failed to create state: {err}. Trying to recover...");
                    self.delete(
                        &application.metadata.name,
                        &device.metadata.name,
                        &token
                    )
                    .await;
                    return CreationOutcome::Failed;
                }
            }
        }
    }

    /// Delete device state.
    ///
    /// This function will shut down the runner in case the state service cannot be contacted,
    /// even after re-trying. And if even that fails, it will **abort** the process.
    pub async fn delete(&self, application: &str, device: &str, token: &str) {
        let mut attempts = self.retry_deletes;
        let delay = Duration::from_millis(250);

        loop {
            match self
                .client
                .delete(&self.session, application, device, token)
                .await
            {
                Ok(_) => break,
                Err(err) => {
                    attempts -= 1;
                    log::error!("Failed to communicate with state service (attempts left: {attempts}): {err}");
                    if attempts == 0 {
                        if let Some(kill_tx) = self.kill_tx.lock().await.take() {
                            let _ = kill_tx.send(());
                            // if we failed to send the kill event, then the shutdown should be
                            // already in progress.
                        } else {
                            // at this point, we couldn't even trigger a normal shutdown, so the
                            // only thing that is left is to abort right away.
                            eprintln!("Unable to contact state service. Last error was: {err}");
                            abort();
                        }
                    } else {
                        sleep(delay).await;
                    }
                }
            }
        }
    }
}

impl CommandRoutingRunner {
    pub async fn run(mut self) -> anyhow::Result<()> {
        let kill_rx = self
            .kill_rx
            .take()
            .ok_or_else(|| anyhow!("Missing kill receiver"))?;

        let looping = async {
            // the first deadline
            let (mut expires, mut deadline) = self.next_deadline(self.expires);
            loop {
                tokio::time::sleep_until(deadline).await;
                (expires, deadline) = self.ping(expires).await?;
            }
        };

        let killed = async {
            if let Ok(()) = kill_rx.await {
                Err(anyhow::anyhow!("Runner got killed"))
            } else {
                // normal shutdown
                Ok(())
            }
        };

        select! {
            rc = looping => rc,
            rc = killed => rc,
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
