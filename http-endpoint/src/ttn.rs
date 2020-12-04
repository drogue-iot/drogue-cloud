use actix_web::{web, HttpResponse};

use drogue_cloud_endpoint_common::error::HttpEndpointError;

use drogue_cloud_endpoint_common::auth::DeviceProperties;
use drogue_cloud_endpoint_common::downstream::{DownstreamSender, Publish};
use drogue_cloud_endpoint_common::error::EndpointError;
use drogue_ttn::http as ttn;

use crate::PublishOptions;

pub async fn publish(
    endpoint: web::Data<DownstreamSender>,
    web::Query(opts): web::Query<PublishOptions>,
    props: Option<DeviceProperties>,
    body: web::Bytes,
) -> Result<HttpResponse, HttpEndpointError> {
    let uplink: ttn::Uplink = serde_json::from_slice(&body).map_err(|err| {
        log::info!("Failed to decode payload: {}", err);
        EndpointError::InvalidFormat {
            source: Box::new(err),
        }
    })?;

    log::info!("Device properties: {:?}", props);

    // eval model_id from query and function port mapping

    let fport = uplink.port.to_string();
    let model_id = opts.model_id.or_else(|| {
        props.as_ref().map(|props| &props.0).and_then(|props| {
            props["lorawan"]["ports"][fport]["model_id"]
                .as_str()
                .map(|str| str.to_string())
        })
    });

    log::info!("Model ID: {:?}", model_id);

    endpoint
        .publish_http(
            Publish {
                channel: uplink.port.to_string(),
                device_id: uplink.dev_id,
                model_id,
                ..Default::default()
            },
            body,
        )
        .await
}
