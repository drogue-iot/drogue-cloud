use crate::{cloudevents_sdk_ntex::request_to_event, App};
use drogue_cloud_endpoint_common::Id;
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

    match Id::from_event(&request_event) {
        Some(device_id) => {
            let device = { app.devices.lock().unwrap().get(&device_id).cloned() };
            if let Some(sender) = device {
                if let Some(command) = request_event.data() {
                    match sender
                        .send(String::try_from(command.clone()).unwrap())
                        .await
                    {
                        Ok(_) => {
                            log::debug!("Command sent to device {:?}", device_id);
                            web::HttpResponse::Ok().finish()
                        }
                        Err(e) => {
                            log::error!("Failed to send a command {:?}", e);
                            web::HttpResponse::BadRequest().finish()
                        }
                    }
                } else {
                    log::error!("Failed to route command: No command provided!");
                    web::HttpResponse::BadRequest().finish()
                }
            } else {
                log::debug!(
                    "Failed to route command: No device {:?} found on this endpoint!",
                    device_id
                );
                web::HttpResponse::Ok().finish()
            }
        }
        None => {
            log::error!("Failed to route command: No device provided!");
            web::HttpResponse::BadRequest().finish()
        }
    }
}
