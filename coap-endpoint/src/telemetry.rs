use crate::{auth::DeviceAuthenticator, downstream::CoapCommandSender, error::CoapEndpointError};
use coap_lite::{CoapRequest, CoapResponse, ContentFormat};
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
                .get_content_format()
                .and_then(|v| content_format_to_media_type(v))
                .map(|s| s.to_string()),
            ..Default::default()
        },
    };

    // Send response
    sender
        .publish_and_await(publish, commands, opts.ct, req)
        .await
}

fn content_format_to_media_type(value: ContentFormat) -> Option<&'static str> {
    match value {
        ContentFormat::TextPlain => Some("text/plain; charset=utf-8"),
        ContentFormat::ApplicationCoseEncrypt0 => {
            Some("application/cose; cose-type=\"cose-encrypt0\"")
        }
        ContentFormat::ApplicationCoseMac0 => Some("application/cose; cose-type=\"cose-emac0\""),
        ContentFormat::ApplicationCoseSign1 => Some("application/cose; cose-type=\"cose-sign1\""),
        ContentFormat::ApplicationAceCbor => Some("application/ace+cbor"),
        ContentFormat::ImageGif => Some("image/gif"),
        ContentFormat::ImageJpeg => Some("image/jpeg"),
        ContentFormat::ImagePng => Some("image/png"),
        ContentFormat::ApplicationLinkFormat => Some("application/link-format"),
        ContentFormat::ApplicationXML => Some("application/xml"),
        ContentFormat::ApplicationOctetStream => Some("application/octet-stream"),
        ContentFormat::ApplicationEXI => Some("application/exi"),
        ContentFormat::ApplicationJSON => Some("application/json"),
        ContentFormat::ApplicationJsonPatchJson => Some("application/json-patch+json"),
        ContentFormat::ApplicationMergePatchJson => Some("application/merge-patch+json"),
        ContentFormat::ApplicationCBOR => Some("application/cbor"),
        ContentFormat::ApplicationCWt => Some("application/cwt"),
        ContentFormat::ApplicationMultipartCore => Some("application/multipart-core"),
        ContentFormat::ApplicationCborSeq => Some("application/cbor-seq"),
        ContentFormat::ApplicationCoseEncrypt => {
            Some("application/cose; cose-type=\"cose-encrypt\"")
        }
        ContentFormat::ApplicationCoseMac => Some("application/cose; cose-type=\"cose-mac\""),
        ContentFormat::ApplicationCoseSign => Some("application/cose; cose-type=\"cose-sign\""),
        ContentFormat::ApplicationCoseKey => Some("application/cose-key"),
        ContentFormat::ApplicationCoseKeySet => Some("application/cose-key-set"),
        ContentFormat::ApplicationSenmlJSON => Some("application/senml+json"),
        ContentFormat::ApplicationSensmlJSON => Some("application/sensml+json"),
        ContentFormat::ApplicationSenmlCBOR => Some("application/senml+cbor"),
        ContentFormat::ApplicationSensmlCBOR => Some("application/sensml+cbor"),
        ContentFormat::ApplicationSenmlExi => Some("application/senml+exi"),
        ContentFormat::ApplicationSensmlExi => Some("application/sensml+exi"),
        ContentFormat::ApplicationYangDataCborSid => Some("application/yang-data+cbor; id=sid"),
        ContentFormat::ApplicationCoapGroupJson => Some("application/coap-group+json"),
        ContentFormat::ApplicationDotsCbor => Some("application/concise-problem-details+cbor"),
        ContentFormat::ApplicationMissingBlocksCborSeq => {
            Some("application/missing-blocks+cbor-seq")
        }
        ContentFormat::ApplicationPkcs7MimeServerGeneratedKey => {
            Some("application/pkcs7-mime; smime-type=server-generated-key")
        }
        ContentFormat::ApplicationPkcs7MimeCertsOnly => {
            Some("application/pkcs7-mime; smime-type=certs-only")
        }
        ContentFormat::ApplicationPkcs8 => Some("application/pkcs8"),
        ContentFormat::ApplicationCsrattrs => Some("application/csrattrs"),
        ContentFormat::ApplicationPkcs10 => Some("application/pkcs10"),
        ContentFormat::ApplicationPkixCert => Some("application/pkix-cert"),
        ContentFormat::ApplicationAifCbor => Some("application/aif+cbor"),
        ContentFormat::ApplicationAifJson => Some("application/aif+json"),
        ContentFormat::ApplicationSenmlXML => Some("application/senml+xml"),
        ContentFormat::ApplicationSensmlXML => Some("application/sensml+xml"),
        ContentFormat::ApplicationSenmlEtchJson => Some("application/senml-etch+json"),
        ContentFormat::ApplicationSenmlEtchCbor => Some("application/senml-etch+cbor"),
        ContentFormat::ApplicationYangDataCbor => Some("application/yang-data+cbor"),
        ContentFormat::ApplicationYangDataCborName => Some("application/yang-data+cbor; id=name"),
        ContentFormat::ApplicationTdJson => Some("application/td+json"),
        ContentFormat::ApplicationVoucherCoseCbor => Some("application/voucher-cose+cbor"),
        ContentFormat::ApplicationVndOcfCbor => Some("application/vnd.ocf+cbor"),
        ContentFormat::ApplicationOscore => Some("application/oscore"),
        ContentFormat::ApplicationJavascript => Some("application/javascript"),
        ContentFormat::ApplicationJsonDeflate => Some("application/json"),
        ContentFormat::ApplicationCborDeflate => Some("application/cbor"),
        ContentFormat::ApplicationVndOmaLwm2mTlv => Some("application/vnd.oma.lwm2m+tlv"),
        ContentFormat::ApplicationVndOmaLwm2mJson => Some("application/vnd.oma.lwm2m+json"),
        ContentFormat::ApplicationVndOmaLwm2mCbor => Some("application/vnd.oma.lwm2m+cbor"),
        ContentFormat::TextCss => Some("text/css"),
        ContentFormat::ImageSvgXml => Some("image/svg+xml"),
        _ => None,
    }
}
