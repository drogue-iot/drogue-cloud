use crate::WebData;
use actix_web::{post, web, HttpRequest, HttpResponse};
use cloudevents_sdk_actix_web::HttpRequestExt;
use drogue_cloud_registry_events::Event;
use serde_json::json;
use std::convert::TryInto;

#[post("/")]
pub async fn events(
    req: HttpRequest,
    payload: web::Payload,
    data: web::Data<WebData>,
) -> Result<HttpResponse, actix_web::error::Error> {
    let event = req.to_event(payload).await;

    log::debug!("Received event: {:?}", event);

    let event = match event?.try_into() {
        Ok(event) => event,
        Err(err) => {
            return Ok(HttpResponse::BadRequest().json(json!({ "details": format!("{}", err) })))
        }
    };

    log::debug!("Registry event: {:?}", event);

    if !is_relevant(&event) {
        // not relevant, consider contacting admin ;-)
        return Ok(HttpResponse::Ok().finish());
    }

    Ok(match data.controller.handle_event(event).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(err) => HttpResponse::InternalServerError().json(json!({
            "details": err.to_string(),
        })),
    })
}

fn is_relevant(event: &Event) -> bool {
    match event {
        Event::Application { path, .. } => path == "." || path == ".spec.ttn",
        _ => false,
    }
}
