use serde::Deserialize;

use crate::command::{command_wait, CommandWait};
use actix_web::{http::header, post, web, HttpResponse};
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    downstream::{DownstreamSender, Outcome, Publish, PublishResponse},
    error::{EndpointError, HttpEndpointError},
    x509::ClientCertificateChain,
};
use drogue_cloud_service_api::auth::{self, ErrorInformation};

#[derive(Deserialize)]
pub struct PublishOptions {
    pub application: Option<String>,
    pub device: Option<String>,
    pub r#as: Option<String>,

    pub model_id: Option<String>,
    pub ttd: Option<u64>,
}

#[post("/{channel}")]
pub async fn publish_plain(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    web::Path(channel): web::Path<String>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    publish(sender, auth, channel, None, opts, req, body, certs).await
}

#[post("/{channel}/{suffix:.*}")]
pub async fn publish_tail(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    web::Path((channel, suffix)): web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    publish(sender, auth, channel, Some(suffix), opts, req, body, certs).await
}

pub async fn publish(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    channel: String,
    suffix: Option<String>,
    opts: PublishOptions,
    req: web::HttpRequest,
    body: web::Bytes,
    certs: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    log::debug!("Publish to '{}'", channel);

    let (application, device) = match auth
        .authenticate_http(
            opts.application,
            opts.device,
            req.headers().get(http::header::AUTHORIZATION),
            certs.map(|c| c.0),
        )
        .await
        .map_err(|err| HttpEndpointError(err.into()))?
        .outcome
    {
        auth::Outcome::Fail => return Err(HttpEndpointError(EndpointError::AuthenticationError)),
        auth::Outcome::Pass {
            application,
            device,
        } => (application, device),
    };

    // If we have an "as" parameter, we publish as another device.
    // FIXME: we need to validate the device as well
    let device_id = match opts.r#as {
        // use the "as" information as device id
        Some(device_id) => device_id,
        // use the original device id
        None => device.metadata.name,
    };

    // publish

    match sender
        .publish(
            Publish {
                channel,
                app_id: application.metadata.name.clone(),
                device_id: device_id.clone(),
                model_id: opts.model_id,
                topic: suffix,
                content_type: req
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string()),
            },
            body,
        )
        .await
    {
        // ok, and accepted
        Ok(PublishResponse {
            outcome: Outcome::Accepted,
        }) => {
            command_wait(
                application.metadata.name,
                device_id,
                CommandWait::from_secs(opts.ttd),
                http::StatusCode::ACCEPTED,
            )
            .await
        }

        // ok, but rejected
        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => {
            command_wait(
                application.metadata.name,
                device_id,
                CommandWait::from_secs(opts.ttd),
                http::StatusCode::NOT_ACCEPTABLE,
            )
            .await
        }

        // internal error
        Err(err) => Ok(HttpResponse::InternalServerError().json(ErrorInformation {
            error: "InternalError".into(),
            message: err.to_string(),
        })),
    }
}
