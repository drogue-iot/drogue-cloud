use crate::command::wait_for_command;
use async_trait::async_trait;
use drogue_client::error::ErrorInformation;
use drogue_cloud_endpoint_common::{
    command::{CommandFilter, Commands},
    error::HttpEndpointError,
    sender::{DownstreamSender, Publish, PublishOutcome, Publisher, DOWNSTREAM_EVENTS_COUNTER},
    sink::Sink,
};
use drogue_cloud_service_api::webapp::{web, HttpResponse};

#[async_trait]
pub trait HttpCommandSender {
    #[allow(clippy::needless_lifetimes)]
    async fn publish_and_await<'a, B>(
        &self,
        publish: Publish<'a>,
        commands: web::Data<Commands>,
        ttd: Option<u64>,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send + Sync;
}

#[async_trait]
impl<S> HttpCommandSender for DownstreamSender<S>
where
    S: Sink + Send + Sync,
    <S as Sink>::Error: Send,
{
    #[allow(clippy::needless_lifetimes)]
    async fn publish_and_await<'a, B>(
        &self,
        publish: Publish<'a>,
        commands: web::Data<Commands>,
        ttd: Option<u64>,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send + Sync,
    {
        let filter = CommandFilter::proxied_device(
            &publish.application.metadata.name,
            &publish.sender_id,
            &publish.device_id,
        );
        match self.publish(publish, body).await {
            // ok, and accepted
            Ok(PublishOutcome::Accepted) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "Accepted"])
                    .inc();
                wait_for_command(commands, filter, ttd).await
            }

            // ok, but rejected
            Ok(PublishOutcome::Rejected) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "Rejected"])
                    .inc();
                Ok(HttpResponse::build(http::StatusCode::NOT_ACCEPTABLE).finish())
            }

            // ok, but rejected
            Ok(PublishOutcome::QueueFull) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "QueueFull"])
                    .inc();
                Ok(HttpResponse::build(http::StatusCode::SERVICE_UNAVAILABLE).finish())
            }

            // internal error
            Err(err) => {
                DOWNSTREAM_EVENTS_COUNTER
                    .with_label_values(&["http", "Error"])
                    .inc();
                Ok(HttpResponse::InternalServerError().json(ErrorInformation {
                    error: "InternalError".into(),
                    message: err.to_string(),
                }))
            }
        }
    }
}
