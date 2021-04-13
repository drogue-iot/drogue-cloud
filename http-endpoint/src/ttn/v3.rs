use crate::{
    telemetry::PublishCommonOptions,
    ttn::{publish_uplink, Uplink},
};
use actix_web::{web, HttpResponse};
use drogue_cloud_endpoint_common::{
    auth::DeviceAuthenticator,
    downstream::DownstreamSender,
    error::{EndpointError, HttpEndpointError},
    x509::ClientCertificateChain,
};
use drogue_ttn::v3::{Message, Payload};

pub async fn publish_v3(
    sender: web::Data<DownstreamSender>,
    auth: web::Data<DeviceAuthenticator>,
    web::Query(opts): web::Query<PublishCommonOptions>,
    req: web::HttpRequest,
    body: web::Bytes,
    cert: Option<ClientCertificateChain>,
) -> Result<HttpResponse, HttpEndpointError> {
    let msg: Message = serde_json::from_slice(&body).map_err(|err| {
        log::info!("Failed to decode payload: {}", err);
        EndpointError::InvalidFormat {
            source: Box::new(err),
        }
    })?;

    let uplink = match msg.payload {
        Payload::Uplink(uplink) => Ok(uplink),
        _ => Err(EndpointError::InvalidRequest {
            details: format!("Invalid message type, expected 'Uplink'"),
        }),
    }?;

    publish_uplink(
        sender,
        auth,
        opts,
        req,
        cert,
        body,
        Uplink {
            device_id: msg.end_device_ids.device_id,
            port: uplink.frame_port.to_string(),
            time: uplink.received_at,
            is_retry: None,
            hardware_address: msg.end_device_ids.dev_addr,
            payload_raw: uplink.frame_payload,
            payload_fields: uplink.decoded_payload.unwrap_or_default(),
        },
    )
    .await
}
