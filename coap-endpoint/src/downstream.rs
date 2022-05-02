use crate::{command::wait_for_command, error::CoapEndpointError};
use async_trait::async_trait;
use coap_lite::{CoapRequest, CoapResponse, ResponseType};
use drogue_cloud_endpoint_common::{
    command::{CommandFilter, Commands},
    error::EndpointError,
    sender::{DownstreamSender, Publish, PublishOutcome, Publisher},
};
use std::net::SocketAddr;

#[async_trait]
pub trait CoapCommandSender {
    async fn publish_and_await<'a>(
        &self,
        publish: Publish<'a>,
        commands: Commands,
        ttd: Option<u64>,
        req: CoapRequest<SocketAddr>,
    ) -> Result<Option<CoapResponse>, CoapEndpointError>;
}

#[async_trait]
impl CoapCommandSender for DownstreamSender {
    async fn publish_and_await<'a>(
        &self,
        publish: Publish<'a>,
        commands: Commands,
        ttd: Option<u64>,
        req: CoapRequest<SocketAddr>,
    ) -> Result<Option<CoapResponse>, CoapEndpointError> {
        let filter = CommandFilter::proxied_device(
            &publish.application.metadata.name,
            &publish.sender.name,
            &publish.device.name,
        );
        match self.publish(publish, &req.message.payload).await {
            // ok, and accepted
            Ok(PublishOutcome::Accepted) => wait_for_command(req, commands, filter, ttd).await,

            // ok, but rejected
            Ok(PublishOutcome::Rejected) => Ok(req.response.map(|mut v| {
                v.set_status(ResponseType::NotAcceptable);
                v
            })),

            // ok, but queue full
            Ok(PublishOutcome::QueueFull) => Ok(req.response.map(|mut v| {
                v.set_status(ResponseType::ServiceUnavailable);
                v
            })),

            // internal error
            Err(err) => Err(CoapEndpointError(EndpointError::ConfigurationError {
                details: err.to_string(),
            })),
        }
    }
}
