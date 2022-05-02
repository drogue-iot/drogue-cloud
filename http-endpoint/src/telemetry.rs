use crate::downstream::HttpCommandSender;
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    command::Commands,
    error::{EndpointError, HttpEndpointError},
    sender::{self, DownstreamSender, PublishIdPair},
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::{
    auth::device::authn,
    webapp::{http::header, web, HttpRequest, HttpResponse},
};
use serde::Deserialize;
use tracing::instrument;

#[derive(Debug, Deserialize)]
pub struct PublishCommonOptions {
    pub application: Option<String>,
    pub device: Option<String>,

    pub data_schema: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PublishOptions {
    #[serde(flatten)]
    pub common: PublishCommonOptions,

    pub r#as: Option<String>,

    #[serde(alias = "commandTimeout")]
    pub ct: Option<u64>,
}

#[allow(clippy::too_many_arguments)]
pub async fn publish_plain(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    channel: web::Path<String>,
    web::Query(opts): web::Query<PublishOptions>,
    req: HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    publish(
        sender,
        auth,
        commands,
        channel.into_inner(),
        None,
        opts,
        req,
        body,
        certs,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn publish_tail(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    path: web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    let (channel, suffix) = path.into_inner();
    publish(
        sender,
        auth,
        commands,
        channel,
        Some(suffix),
        opts,
        req,
        body,
        certs,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip(downstream, auth, commands, body))]
pub async fn publish(
    downstream: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    channel: String,
    suffix: Option<String>,
    opts: PublishOptions,
    req: HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    log::debug!("Publish to '{}'", channel);

    let (application, device, r#as) = match auth
        .authenticate_http(
            opts.common.application,
            opts.common.device,
            req.headers().get(http::header::AUTHORIZATION),
            certs.map(|c| c.0),
            opts.r#as.clone(),
        )
        .await
        .map_err(|err| HttpEndpointError(err.into()))?
        .outcome
    {
        authn::Outcome::Fail => return Err(HttpEndpointError(EndpointError::AuthenticationError)),
        authn::Outcome::Pass {
            application,
            device,
            r#as,
        } => (application, device, r#as),
    };

    let PublishIdPair { device, sender } = PublishIdPair::with_devices(device, r#as);

    // publish

    let publish = sender::Publish {
        channel,
        application: &application,
        device,
        sender,
        options: sender::PublishOptions {
            data_schema: opts.common.data_schema,
            topic: suffix,
            content_type: req
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string()),
            ..Default::default()
        },
    };

    downstream
        .publish_and_await(publish, commands, opts.ct, body)
        .await
}
