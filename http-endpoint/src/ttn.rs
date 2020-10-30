use actix_web::{post, web, HttpResponse};
use futures::StreamExt;

use crate::error::HttpEndpointError;

use drogue_cloud_endpoint_common::downstream::{
    DownstreamSender, Outcome, Publish, PublishResponse,
};
use drogue_cloud_endpoint_common::error::EndpointError;
use drogue_ttn::http as ttn;

use log;

#[post("/ttn")]
pub async fn publish(
    endpoint: web::Data<DownstreamSender>,
    mut body: web::Payload,
) -> Result<HttpResponse, HttpEndpointError> {
    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let bytes = bytes.freeze();

    let uplink: ttn::Uplink = serde_json::from_slice(&bytes).map_err(|err| {
        log::info!("Failed to decode payload: {}", err);
        EndpointError::InvalidFormat {
            source: Box::new(err),
        }
    })?;

    match endpoint
        .publish(
            Publish {
                channel: uplink.port.to_string(),
                device_id: uplink.dev_id,
            },
            bytes,
        )
        .await
    {
        // ok, and accepted
        Ok(PublishResponse {
            outcome: Outcome::Accepted,
        }) => Ok(HttpResponse::Accepted().finish()),

        // ok, but rejected
        Ok(PublishResponse {
            outcome: Outcome::Rejected,
        }) => Ok(HttpResponse::NotAcceptable().finish()),

        // internal error
        Err(err) => Ok(HttpResponse::InternalServerError()
            .content_type("text/plain")
            .body(err.to_string())),
    }
}
