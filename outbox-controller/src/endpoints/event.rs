use crate::WebData;
use actix_web::{post, web, HttpRequest, HttpResponse};
use cloudevents::actix::HttpRequestExt;
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_registry_events::Event;
use std::convert::TryInto;

#[post("/")]
pub async fn events(
    req: HttpRequest,
    payload: web::Payload,
    data: web::Data<WebData>,
) -> Result<HttpResponse, actix_web::Error> {
    let event = req.to_event(payload).await;

    log::debug!("Received event: {:?}", event);

    Ok(mark_seen(event?, data).await?)
}

async fn mark_seen(
    event: cloudevents::event::Event,
    data: web::Data<WebData>,
) -> Result<HttpResponse, ServiceError> {
    let event: Event = event
        .try_into()
        .map_err(|err| ServiceError::BadRequest(format!("Failed to parse event: {}", err)))?;

    log::debug!("Outbox event: {:?}", event);

    data.service.mark_seen(event).await?;

    Ok(HttpResponse::Ok().finish())
}
