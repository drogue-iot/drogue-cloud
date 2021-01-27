use crate::command::{command_wait, CommandWait};
use actix_web::HttpResponse;
use async_trait::async_trait;
use drogue_cloud_endpoint_common::downstream::{DownstreamSender, Outcome};
use drogue_cloud_endpoint_common::{downstream::Publish, error::HttpEndpointError};
use http::StatusCode;

#[async_trait]
pub trait HttpCommandSender {
    async fn publish_and_await<B>(
        &self,
        publish: Publish,
        command: CommandWait,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send;
}

#[async_trait]
impl HttpCommandSender for DownstreamSender {
    async fn publish_and_await<B>(
        &self,
        publish: Publish,
        command: CommandWait,
        body: B,
    ) -> Result<HttpResponse, HttpEndpointError>
    where
        B: AsRef<[u8]> + Send,
    {
        self.publish_http(publish.clone(), body, |outcome| async move {
            command_wait(
                &publish.tenant_id,
                &publish.device_id,
                command,
                match outcome {
                    // FIXME: we need to distinguish between: with or without command
                    Outcome::Accepted => StatusCode::ACCEPTED,
                    Outcome::Rejected => StatusCode::NOT_ACCEPTABLE,
                },
            )
            .await
        })
        .await
    }
}
