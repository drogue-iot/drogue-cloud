use serde::Deserialize;

use crate::command::wait_for_command;
use actix_web::{http::header, post, web, HttpResponse};
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    commands::Commands,
    downstream::{self, DownstreamSender, Outcome, PublishResponse},
    error::{EndpointError, HttpEndpointError},
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::auth::{authn, ErrorInformation};
use drogue_cloud_service_common::Id;

#[derive(Deserialize)]
pub struct PublishCommonOptions {
    pub application: Option<String>,
    pub device: Option<String>,

    pub data_schema: Option<String>,
}

#[derive(Deserialize)]
pub struct PublishOptions {
    #[serde(flatten)]
    pub common: PublishCommonOptions,

    pub r#as: Option<String>,

    #[serde(alias = "commandTimeout")]
    pub ct: Option<u64>,
}

#[post("/{channel}")]
pub async fn publish_plain(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    channel: web::Path<String>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
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

#[post("/{channel}/{suffix:.*}")]
pub async fn publish_tail(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    path: web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
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

pub async fn publish(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    commands: web::Data<Commands>,
    channel: String,
    suffix: Option<String>,
    opts: PublishOptions,
    req: web::HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    log::debug!("Publish to '{}'", channel);

    let (application, device, _) = match auth
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
    let device_id = match opts.r#as {
        // use the "as" information as device id
        Some(device_id) => device_id,
        // use the original device id
        None => device.metadata.name,
    };

    // publish

    match sender
        .publish(
            downstream::Publish {
                channel,
                app_id: application.metadata.name.clone(),
                device_id: device_id.clone(),
                options: downstream::PublishOptions {
                    data_schema: opts.common.data_schema,
                    topic: suffix,
                    content_type: req
                        .headers()
                        .get(header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string()),
                    ..Default::default()
                },
            },
            body,
        )
        .await
    {
        // ok, and accepted
        Ok(PublishResponse {
            outcome: Outcome::Accepted,
        }) => {
            wait_for_command(
                commands,
                Id::new(application.metadata.name, device_id),
                opts.ct,
            )
            .await
        }

        // ok, but rejected
        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => Ok(HttpResponse::build(http::StatusCode::NOT_ACCEPTABLE).finish()),

        // internal error
        Err(err) => Ok(HttpResponse::InternalServerError().json(ErrorInformation {
            error: "InternalError".into(),
            message: err.to_string(),
        })),
    }
}
