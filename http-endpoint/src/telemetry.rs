use serde::Deserialize;

use crate::command::command_wait;
use actix_web::{http::header, post, web, HttpResponse};
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    downstream::{DownstreamSender, Outcome, Publish, PublishResponse},
    error::EndpointError,
    error::HttpEndpointError,
};
use drogue_cloud_service_api::auth;

#[derive(Deserialize)]
pub struct PublishOptions {
    tenant: Option<String>,
    device: Option<String>,
    r#as: Option<String>,

    model_id: Option<String>,
    ttd: Option<u64>,
}

#[post("/{channel}")]
pub async fn publish_plain(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    web::Path(channel): web::Path<String>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    publish(sender, auth, channel, None, opts, req, body).await
}

#[post("/{channel}/{suffix:.*}")]
pub async fn publish_tail(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    web::Path((channel, suffix)): web::Path<(String, String)>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    publish(sender, auth, channel, Some(suffix), opts, req, body).await
}

pub async fn publish(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    channel: String,
    _suffix: Option<String>,
    opts: PublishOptions,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    log::debug!("Publish to '{}'", channel);

    let (tenant, device) = match auth
        .authenticate_http(
            opts.tenant,
            opts.device,
            req.headers().get(http::header::AUTHORIZATION),
        )
        .await
        .map_err(|err| HttpEndpointError(err.into()))?
        .outcome
    {
        auth::Outcome::Fail => {
            return Err(HttpEndpointError(EndpointError::AuthenticationError).into())
        }
        auth::Outcome::Pass { tenant, device } => (tenant, device),
    };

    // If we have an "as" parameter, we publish as another device.
    // FIXME: we need to validate the device as well
    let device_id = match opts.r#as {
        // use the "as" information as device id
        Some(device_id) => device_id,
        // use the original device id
        None => device.id,
    };

    // publish

    match sender
        .publish(
            Publish {
                channel,
                tenant_id: tenant.id.clone(),
                device_id: device_id.clone(),
                model_id: opts.model_id,
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
        }) => command_wait(tenant.id, device_id, opts.ttd, http::StatusCode::ACCEPTED).await,

        // ok, but rejected
        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => {
            command_wait(
                tenant.id,
                device_id,
                opts.ttd,
                http::StatusCode::NOT_ACCEPTABLE,
            )
            .await
        }

        // internal error
        Err(err) => Ok(HttpResponse::InternalServerError()
            .content_type("text/plain")
            .body(err.to_string())),
    }
}
