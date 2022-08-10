use crate::service::OutboxService;
use drogue_cloud_database_common::{error::ServiceError, models::outbox::OutboxEntry};
use drogue_cloud_registry_events::{Event, EventSender, EventSenderError};
use drogue_cloud_service_common::app::{Startup, StartupExt};
use futures::TryStreamExt;
use lazy_static::lazy_static;
use prometheus::{register_int_counter_vec, IntCounterVec};
use std::{sync::Arc, time::Duration};
use tokio::time::MissedTickBehavior;

lazy_static! {
    static ref RESENT_EVENTS: IntCounterVec = register_int_counter_vec!(
        "drogue_registry_events_resent",
        "Events which have been resent",
        &["result"]
    )
    .unwrap();
}

/// Re-send missed outbox events.
///
/// The `Resender` will poll the outbox every `interval` and look for entries older than `before`.
/// It will then re-send them using the configured [`EventSender`]. It will not mark the entries
/// as seen, as this wil be done through the normal flow of events.
pub struct Resender<S>
where
    S: EventSender,
{
    pub interval: Duration,
    pub before: chrono::Duration,
    pub service: Arc<OutboxService>,
    pub sender: Arc<S>,
}

impl<S> Resender<S>
where
    S: EventSender + 'static,
{
    pub fn start(self, startup: &mut dyn Startup) {
        startup.spawn(self.run());
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(self.interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            self.tick().await;
        }
    }

    async fn tick(&self) {
        match self.process().await {
            Ok(_) => {
                log::debug!("Completed resend operation")
            }
            Err(err) => {
                log::debug!("Resend operation failed: {}", err);
            }
        }
    }

    async fn send_entry(&self, entry: OutboxEntry) -> Result<(), EventSenderError<S::Error>> {
        let event: Event = entry.into();
        match self.sender.notify(Some(event)).await {
            Ok(result) => {
                RESENT_EVENTS.with_label_values(&["ok"]).inc();
                Ok(result)
            }
            Err(err) => {
                RESENT_EVENTS.with_label_values(&["err"]).inc();
                Err(err)
            }
        }
    }
    async fn process(&self) -> Result<(), ServiceError> {
        let mut stream = self.service.retrieve_unseen(self.before).await?;

        let mut n = 0;

        loop {
            match stream.try_next().await {
                Ok(Some(entry)) => self.send_entry(entry).await.map_err(|err| {
                    ServiceError::Internal(format!("Failed to send event (n = {}): {}", n, err))
                })?,
                Ok(None) => {
                    break;
                }
                Err(err) => {
                    log::info!("Failed to retrieve next outbox entry (n = {}): {}", n, err);
                    return Err(err);
                }
            }
            n += 1;
        }

        if n > 0 {
            log::info!("Processed {} missed outbox entries", n);
        } else {
            log::debug!("No outbox entries found to process");
        }

        Ok(())
    }
}
