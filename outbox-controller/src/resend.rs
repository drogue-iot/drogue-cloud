use crate::service::OutboxService;
use actix::prelude::*;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_database_common::models::outbox::OutboxEntry;
use drogue_cloud_registry_events::{Event, EventSender, EventSenderError};
use futures::TryStreamExt;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

#[derive(Message)]
#[rtype(result = "Result<(), Infallible>")]
struct MsgResend;

struct ResendContext<S>
where
    S: EventSender,
{
    pub service: Arc<OutboxService>,
    pub sender: Arc<S>,
    pub before: chrono::Duration,
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

impl<S> Actor for Resender<S>
where
    S: EventSender + 'static,
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.trigger(ctx);
    }
}

impl<S> Resender<S>
where
    S: EventSender + 'static,
{
    fn trigger(&self, ctx: &mut Context<Self>) {
        log::debug!("Trigger next: {:?}", self.interval);
        ctx.notify_later(MsgResend, self.interval);
    }

    async fn execute(ctx: ResendContext<S>) {
        match Self::process(ctx).await {
            Ok(_) => {
                log::debug!("Completed resend operation")
            }
            Err(err) => {
                log::debug!("Resend operation failed: {}", err);
            }
        }
    }

    async fn process(ctx: ResendContext<S>) -> Result<(), ServiceError> {
        let mut stream = ctx.service.retrieve_unseen(ctx.before).await?;

        let mut n = 0;

        loop {
            match stream.try_next().await {
                Ok(Some(entry)) => Self::send_entry(entry, &ctx).await.map_err(|err| {
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

    async fn send_entry(
        entry: OutboxEntry,
        ctx: &ResendContext<S>,
    ) -> Result<(), EventSenderError<S::Error>> {
        let event: Event = entry.into();
        ctx.sender.notify(Some(event)).await?;
        Ok(())
    }
}

impl<S> Handler<MsgResend> for Resender<S>
where
    S: EventSender + 'static,
{
    type Result = ResponseActFuture<Self, Result<(), Infallible>>;

    fn handle(&mut self, _: MsgResend, _: &mut Context<Self>) -> Self::Result {
        log::debug!("Process resend");

        let ctx = ResendContext {
            service: self.service.clone(),
            sender: self.sender.clone(),
            before: self.before,
        };

        Box::pin(
            async {
                Self::execute(ctx).await;
            }
            .into_actor(self)
            .map(|_, this, ctx| {
                // whatever happened, we re-schedule
                this.trigger(ctx);
                Ok(())
            }),
        )
    }
}
