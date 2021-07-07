use crate::WebData;
use actix_web::{post, web, HttpResponse};
use drogue_cloud_database_common::error::ServiceError;
use drogue_cloud_registry_events::Event;
use std::convert::TryInto;

#[post("/")]
pub async fn events(
    event: cloudevents::Event,
    data: web::Data<WebData>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Received event: {:?}", event);
    Ok(mark_seen(event, data).await?)
}

async fn mark_seen(
    event: cloudevents::Event,
    data: web::Data<WebData>,
) -> Result<HttpResponse, ServiceError> {
    let event: Event = event
        .try_into()
        .map_err(|err| ServiceError::BadRequest(format!("Failed to parse event: {}", err)))?;

    log::debug!("Outbox event: {:?}", event);

    data.service.mark_seen(event).await?;

    Ok(HttpResponse::Ok().finish())
}
