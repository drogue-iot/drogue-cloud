use crate::WebData;
use actix_web::{post, web, HttpRequest, HttpResponse};
use cloudevents::binding::actix::HttpRequestExt;
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

    Ok(match is_relevant(event) {
        Some((app, None)) => match data.controller.handle_app_event(app).await {
            Ok(_) => HttpResponse::Ok().finish(),
            Err(err) => HttpResponse::InternalServerError().json(json!({
                "details": err.to_string(),
            })),
        },
        Some((app, Some(device))) => match data.controller.handle_device_event(app, device).await {
            Ok(_) => HttpResponse::Ok().finish(),
            Err(err) => HttpResponse::InternalServerError().json(json!({
                "details": err.to_string(),
            })),
        },
        _ => {
            // not relevant, consider contacting admin ;-)
            HttpResponse::Ok().finish()
        }
    })
}

fn is_relevant(event: Event) -> Option<(String, Option<String>)> {
    match event {
        Event::Application {
            path, application, ..
        } if path == "." || path == ".metadata" || path == ".spec.ttn" => Some((application, None)),
        Event::Device {
            path,
            application,
            device,
            ..
        } if path == "."
            || path == ".metadata"
            || path == ".spec.ttn"
            || path == ".spec.gatewaySelector" =>
        {
            Some((application, Some(device)))
        }
        _ => None,
    }
}
