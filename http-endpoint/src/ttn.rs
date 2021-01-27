use crate::command::CommandWait;
use crate::downstream::HttpCommandSender;
use actix_web::{put, web, HttpResponse};
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    downstream::{DownstreamSender, Publish},
    error::{EndpointError, HttpEndpointError},
};
use drogue_cloud_service_api::auth;
use drogue_ttn::http as ttn;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct PublishOptions {
    pub tenant: Option<String>,
    pub device: Option<String>,

    pub model_id: Option<String>,
    pub ttd: Option<u64>,
}

#[put("")]
pub async fn publish(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    web::Query(opts): web::Query<PublishOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
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
        auth::Outcome::Fail => return Err(HttpEndpointError(EndpointError::AuthenticationError)),
        auth::Outcome::Pass { tenant, device } => (tenant, device),
    };

    let uplink: ttn::Uplink = serde_json::from_slice(&body).map_err(|err| {
        log::info!("Failed to decode payload: {}", err);
        EndpointError::InvalidFormat {
            source: Box::new(err),
        }
    })?;

    log::info!("Tenant / Device properties: {:?} / {:?}", tenant, device);

    // eval model_id from query and function port mapping
    let model_id = opts.model_id.or_else(|| {
        let fport = uplink.port.to_string();
        device.data.properties["lorawan"]["ports"][fport]["model_id"]
            .as_str()
            .map(|str| str.to_string())
    });

    let device_id = uplink.dev_id;

    log::info!("Device ID: {}, Model ID: {:?}", device_id, model_id);

    // FIXME: need to authorize device

    sender
        .publish_and_await(
            Publish {
                channel: uplink.port.to_string(),
                device_id,
                model_id,
                ..Default::default()
            },
            CommandWait::from_secs(opts.ttd),
            body,
        )
        .await
}
