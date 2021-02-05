use crate::{cloudevents_sdk_ntex::request_to_event, App};
use cloudevents::event::ExtensionValue;
use drogue_cloud_endpoint_common::command_router::Id;
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

    let app_id_ext = request_event.extension("application");
    let device_id_ext = request_event.extension("device");

    match (app_id_ext, device_id_ext) {
        (Some(ExtensionValue::String(app_id)), Some(ExtensionValue::String(device_id))) => {
            let device = {
                app.devices
                    .lock()
                    .unwrap()
                    .get(&Id::new(app_id, device_id))
                    .cloned()
            };
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
        _ => {
            log::error!("Failed to route command: No device provided!");
            web::HttpResponse::BadRequest().finish()
        }
    }
}
