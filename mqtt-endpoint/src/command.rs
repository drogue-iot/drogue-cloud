use crate::{cloudevents_sdk_ntex::request_to_event, App};
use drogue_cloud_endpoint_common::commands::Command;
use ntex::{http, web};
use std::convert::TryFrom;

#[web::post("/command-service")]
pub async fn command_service(
    req: web::HttpRequest,
    payload: web::types::Payload,
    app: web::types::Data<App>,
) -> http::Response {
    log::debug!("Command request: {:?}", req);

    let request_event = request_to_event(&req, payload).await.unwrap();

    match Command::try_from(request_event.clone()) {
        Ok(command) => {
            if let Err(e) = app.commands.send(command).await {
                log::error!("Failed to route command: {}", e);
                web::HttpResponse::BadRequest().finish()
            } else {
                web::HttpResponse::Ok().finish()
            }
        }
        Err(_) => {
            log::error!("No device-id provided");
            web::HttpResponse::BadRequest().finish()
        }
    }
}
