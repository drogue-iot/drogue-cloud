use actix_web::{post, web, HttpResponse};
use drogue_cloud_endpoint_common::downstream::{
    DownstreamSender, Outcome, Publish, PublishResponse,
};
use futures::StreamExt;

#[post("/ttn")]
pub async fn publish(
    endpoint: web::Data<DownstreamSender>,
    mut body: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let mut bytes = web::BytesMut::new();
    while let Some(item) = body.next().await {
        bytes.extend_from_slice(&item?);
    }
    let bytes = bytes.freeze();

    match endpoint
        .publish(
            Publish {
                channel: "ttn".into(),
                device_id: "4711".into(),
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
