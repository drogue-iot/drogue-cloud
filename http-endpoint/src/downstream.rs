use crate::command::wait_for_command;
use actix_web::{web, HttpResponse};
use async_trait::async_trait;
use drogue_client::error::ErrorInformation;
use drogue_cloud_endpoint_common::downstream::PublishOutcome;
use drogue_cloud_endpoint_common::{
    commands::Commands,
    downstream::{DownstreamSender, DownstreamSink, Publish},
    error::HttpEndpointError,
};
use drogue_cloud_service_common::Id;

#[async_trait]
pub trait HttpCommandSender {
    async fn publish_and_await<B>(
        &self,
        publish: Publish,
        commands: web::Data<Commands>,
        ttd: Option<u64>,
        //command: CommandWait,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send;
}

#[async_trait]
impl<S> HttpCommandSender for DownstreamSender<S>
where
    S: DownstreamSink + Send + Sync,
    <S as DownstreamSink>::Error: Send,
{
    async fn publish_and_await<B>(
        &self,
        publish: Publish,
        commands: web::Data<Commands>,
        ttd: Option<u64>,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send,
    {
        let id = Id::new(&publish.app_id, &publish.device_id);
        match self.publish(publish, body).await {
            // ok, and accepted
            Ok(PublishOutcome::Accepted) => wait_for_command(commands, id, ttd).await,

            // ok, but rejected
            Ok(PublishOutcome::Rejected) => {
                Ok(HttpResponse::build(http::StatusCode::NOT_ACCEPTABLE).finish())
            }

            // ok, but rejected
            Ok(PublishOutcome::QueueFull) => {
                Ok(HttpResponse::build(http::StatusCode::SERVICE_UNAVAILABLE).finish())
            }

            // internal error
            Err(err) => Ok(HttpResponse::InternalServerError().json(ErrorInformation {
                error: "InternalError".into(),
                message: err.to_string(),
            })),
        }
    }
}
