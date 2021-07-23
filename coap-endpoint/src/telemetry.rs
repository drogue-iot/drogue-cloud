use crate::auth::DeviceAuthenticator;
use crate::downstream::CoapCommandSender;
use crate::error::CoapEndpointError;
use coap_lite::{CoapOption, CoapRequest, CoapResponse};
use drogue_cloud_endpoint_common::{
    command::Commands,
    downstream::{self, DownstreamSender, DownstreamSink},
    error::EndpointError,
};
use drogue_cloud_service_api::auth::device::authn;
use http::HeaderValue;
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Debug, Deserialize, PartialEq)]
pub struct PublishCommonOptions {
    pub application: Option<String>,
    pub device: Option<String>,

    pub data_schema: Option<String>,
}

impl Default for PublishCommonOptions {
    fn default() -> Self {
        PublishCommonOptions {
            application: None,
            device: None,
            data_schema: None,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct PublishOptions {
    #[serde(flatten)]
    pub common: PublishCommonOptions,

    pub r#as: Option<String>,

    #[serde(alias = "commandTimeout")]
    pub ct: Option<u64>,
}

impl Default for PublishOptions {
    fn default() -> Self {
        PublishOptions {
            common: PublishCommonOptions::default(),
            r#as: None,
            ct: None,
        }
    }
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
    S: DownstreamSink + Send,
    <S as DownstreamSink>::Error: Send,
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
    S: DownstreamSink + Send,
    <S as DownstreamSink>::Error: Send,
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
    S: DownstreamSink + Send,
    <S as DownstreamSink>::Error: Send,
{
    log::debug!("Publish to '{}'", channel);

    let (application, device, _) = match authenticator
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
    let device_id = match opts.r#as {
        // use the "as" information as device id
        Some(device_id) => device_id,
        // use the original device id
        None => device.metadata.name,
    };

    // publish

    let publish = downstream::Publish {
        channel,
        app_id: application.metadata.name.clone(),
        device_id: device_id.clone(),
        options: downstream::PublishOptions {
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

    sender
        .publish_and_await(publish, commands, opts.ct, req.message.payload.clone(), req)
        .await
}
