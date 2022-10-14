use crate::{auth::DeviceAuthenticator, downstream::CoapCommandSender, error::CoapEndpointError};
use coap_lite::{CoapOption, CoapRequest, CoapResponse};
use drogue_cloud_endpoint_common::{
    command::Commands,
    error::EndpointError,
    psk::VerifiedIdentity,
    sender::{self, DownstreamSender, ToPublishId},
    x509::ClientCertificateChain,
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

pub async fn publish_plain(
    sender: DownstreamSender,
    authenticator: DeviceAuthenticator,
    commands: Commands,
    channel: String,
    opts: PublishOptions,
    req: CoapRequest<SocketAddr>,
    auth: Option<&Vec<u8>>,
    certs: Option<ClientCertificateChain>,
    verified_identity: Option<VerifiedIdentity>,
) -> Result<Option<CoapResponse>, CoapEndpointError> {
    publish(
        sender,
        authenticator,
        commands,
        channel,
        None,
        opts,
        req,
        auth,
        certs,
        verified_identity,
    )
    .await
}

pub async fn publish_tail(
    sender: DownstreamSender,
    authenticator: DeviceAuthenticator,
    commands: Commands,
    path: (String, String),
    opts: PublishOptions,
    req: CoapRequest<SocketAddr>,
    auth: Option<&Vec<u8>>,
    certs: Option<ClientCertificateChain>,
    verified_identity: Option<VerifiedIdentity>,
) -> Result<Option<CoapResponse>, CoapEndpointError> {
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
        certs,
        verified_identity,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn publish(
    sender: DownstreamSender,
    authenticator: DeviceAuthenticator,
    commands: Commands,
    channel: String,
    suffix: Option<String>,
    opts: PublishOptions,
    req: CoapRequest<SocketAddr>,
    auth: Option<&Vec<u8>>,
    certs: Option<ClientCertificateChain>,
    verified_identity: Option<VerifiedIdentity>,
) -> Result<Option<CoapResponse>, CoapEndpointError> {
    log::debug!("Publish to '{}'", channel);

    if let Some(auth) = &auth {
        log::debug!(
            "Auth header: {:?}",
            HeaderValue::from_bytes(auth).as_ref().ok()
        );
    }
    let (application, device, r#as) = match authenticator
        .authenticate_coap(
            opts.common.application,
            opts.common.device,
            auth.map(|a| HeaderValue::from_bytes(a).ok())
                .flatten()
                .as_ref(),
            certs,
            verified_identity,
        )
        .await
        .map_err(|err| CoapEndpointError(err.into()))?
        .outcome
    {
        authn::Outcome::Fail => {
            return Err(CoapEndpointError(EndpointError::AuthenticationError));
        }
        authn::Outcome::Pass {
            application,
            device,
            r#as,
        } => (application, device.metadata.name, r#as),
    };

    // If we have an "as" parameter, we publish as another device.
    let (sender_id, device_id) = match r#as {
        // use the "as" information as device id
        Some(r#as) => (device.to_id(), (&r#as.metadata).to_id()),
        // use the original device id
        None => (device.to_id(), device.to_id()),
    };

    // Create Publish Object
    let publish = sender::Publish {
        channel,
        application: &application,
        device: device_id,
        sender: sender_id,
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
