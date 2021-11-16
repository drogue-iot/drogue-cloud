use crate::service::Service;
use crate::wshandler::WsHandler;

use actix::Addr;
use actix_web::web::Payload;
use actix_web::{get, web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct GroupId {
    group_id: Option<String>,
}

#[get("")]
pub async fn start_connection(
    req: HttpRequest,
    stream: Payload,
    application: web::Path<String>,
    service_addr: web::Data<Addr<Service>>,
    web::Query(group_id): web::Query<GroupId>,
) -> Result<HttpResponse, Error> {
    log::info!("STARTING WS CONNECTION");
    let application = application.into_inner();

    log::info!("STARTING WS CONNECTION");
    // launch web socket actor
    let ws = WsHandler::new(
        application,
        group_id.group_id,
        service_addr.get_ref().clone(),
    );
    let resp = ws::start(ws, &req, stream)?;
    Ok(resp)
}
