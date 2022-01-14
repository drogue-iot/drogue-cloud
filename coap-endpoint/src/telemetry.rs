use crate::{auth::DeviceAuthenticator, downstream::CoapCommandSender, error::CoapEndpointError};
use coap_lite::{CoapOption, CoapRequest, CoapResponse};
use drogue_cloud_endpoint_common::{
    command::Commands,
    error::EndpointError,
    sender::{self, DownstreamSender},
    sink::Sink,
};
use drogue_cloud_service_api::auth::device::authn;
use http::HeaderValue;
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Debug, Deserialize, PartialEq, Default)]
pub struct PublishCommonOptions {
    pub application: Option<String>,
    pub device: Option<String>,

    pub data_schema: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Default)]
pub struct PublishOptions {
    #[serde(flatten)]
    pub common: PublishCommonOptions,

    pub r#as: Option<String>,

    #[serde(alias = "commandTimeout")]
    pub ct: Option<u64>,
}

pub async fn publish_plain<S>(
    sender: DownstreamSender<S>,
    authenticator: DeviceAuthenticator,
    commands: Commands,
    channel: String,
    opts: PublishOptions,
    req: CoapRequest<SocketAddr>,
    auth: &[u8],
) -> Result<Option<CoapResponse>, CoapEndpointError>
where
    S: Sink + Send,
    <S as Sink>::Error: Send,
{
    publish(
        sender,
        authenticator,
        commands,
        channel,
        None,
        opts,
        req,
        auth,
    )
    .await
}

pub async fn publish_tail<S>(
    sender: DownstreamSender<S>,
    authenticator: DeviceAuthenticator,
    commands: Commands,
    path: (String, String),
    opts: PublishOptions,
    req: CoapRequest<SocketAddr>,
    auth: &[u8],
) -> Result<Option<CoapResponse>, CoapEndpointError>
where
    S: Sink + Send,
    <S as Sink>::Error: Send,
{
    let (channel, suffix) = path;
    publish(
        sender,
        authenticator,
        commands,
        channel,
        Some(suffix),
        opts,
        req,
        auth,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn publish<S>(
    sender: DownstreamSender<S>,
    authenticator: DeviceAuthenticator,
    commands: Commands,
    channel: String,
    suffix: Option<String>,
    opts: PublishOptions,
    req: CoapRequest<SocketAddr>,
    auth: &[u8],
) -> Result<Option<CoapResponse>, CoapEndpointError>
where
    S: Sink + Send,
    <S as Sink>::Error: Send,
{
    log::debug!("Publish to '{}'", channel);

    let (application, device, r#as) = match authenticator
        .authenticate_coap(
            opts.common.application,
            opts.common.device,
            HeaderValue::from_bytes(auth).as_ref().ok(),
        )
        .await
        .map_err(|err| CoapEndpointError(err.into()))?
        .outcome
    {
        authn::Outcome::Fail => return Err(CoapEndpointError(EndpointError::AuthenticationError)),
        authn::Outcome::Pass {
            application,
            device,
            r#as,
        } => (application, device, r#as),
    };

    // If we have an "as" parameter, we publish as another device.
    let (sender_id, device_id) = match r#as {
        // use the "as" information as device id
        Some(r#as) => (device.metadata.name, r#as.metadata.name),
        // use the original device id
        None => (device.metadata.name.clone(), device.metadata.name),
    };

    // Create Publish Object
    let publish = sender::Publish {
        channel,
        application: &application,
        device_id,
        sender_id,
        options: sender::PublishOptions {
            data_schema: opts.common.data_schema,
            topic: suffix,
            content_type: req
                .message
                .get_option(CoapOption::ContentFormat)
                .and_then(|v| std::str::from_utf8(v.front().unwrap()).ok())
                .map(|s| s.to_string()),
            ..Default::default()
        },
    };

    // Send response
    sender
        .publish_and_await(publish, commands, opts.ct, req)
        .await
}
