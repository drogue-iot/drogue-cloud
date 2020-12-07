use actix_web::{post, web, HttpResponse};

use drogue_cloud_endpoint_common::error::HttpEndpointError;

use drogue_cloud_endpoint_common::downstream::{DownstreamSender, Publish};
use drogue_cloud_endpoint_common::error::EndpointError;
use drogue_ttn::http as ttn;

use crate::basic_auth::DeviceProperties;
use crate::PublishOptions;

#[post("/ttn")]
pub async fn publish(
    endpoint: web::Data<DownstreamSender>,
    web::Query(opts): web::Query<PublishOptions>,
    device_properties: Option<DeviceProperties>,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    let uplink: ttn::Uplink = serde_json::from_slice(&body).map_err(|err| {
        log::info!("Failed to decode payload: {}", err);
        EndpointError::InvalidFormat {
            source: Box::new(err),
        }
    })?;

    log::info!("Device properties: {:?}", device_properties);

    endpoint
        .publish_http(
            Publish {
                channel: uplink.port.to_string(),
                device_id: uplink.dev_id,
                model_id: opts.model_id,
                ..Default::default()
            },
            body,
        )
        .await
}
