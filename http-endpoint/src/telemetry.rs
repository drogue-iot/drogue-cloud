use crate::downstream::HttpCommandSender;
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    command::Commands,
    error::{EndpointError, HttpEndpointError},
    sender::{self, DownstreamSender},
    sink::Sink,
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::auth::device::authn;
use drogue_cloud_service_api::webapp::{http::header, web, HttpResponse};
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
pub async fn publish_plain<S>(
    sender: web::Data<DownstreamSender<S>>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    channel: web::Path<String>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError>
where
    S: Sink,
{
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
pub async fn publish_tail<S>(
    sender: web::Data<DownstreamSender<S>>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    path: web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError>
where
    S: Sink,
{
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
#[instrument(skip(sender, auth, commands))]
pub async fn publish<S>(
    sender: web::Data<DownstreamSender<S>>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    channel: String,
    suffix: Option<String>,
    opts: PublishOptions,
    req: web::HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError>
where
    S: Sink + Send,
    <S as Sink>::Error: Send,
{
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

    // If we have an "as" parameter, we publish as another device.
    let (sender_id, device_id) = match r#as {
        // use the "as" information as device id
        Some(r#as) => (device.metadata.name, r#as.metadata.name),
        // use the original device id
        None => (device.metadata.name.clone(), device.metadata.name),
    };

    // publish

    let publish = sender::Publish {
        channel,
        application: &application,
        device_id,
        sender_id,
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

    sender
        .publish_and_await(publish, commands, opts.ct, body)
        .await
}
