use crate::{service::Service, wshandler::WsHandler};
use actix::Addr;
use actix_web::{
    web::{self, Payload},
    Error, HttpRequest, HttpResponse,
};
use actix_web_actors::ws;
use drogue_client::openid::OpenIdTokenProvider;
use drogue_cloud_service_api::webapp as actix_web;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct GroupId {
    group_id: Option<String>,
}

pub async fn start_connection(
    req: HttpRequest,
    stream: Payload,
    application: web::Path<String>,
    service_addr: web::Data<Addr<Service<Option<OpenIdTokenProvider>>>>,
    web::Query(group_id): web::Query<GroupId>,
) -> Result<HttpResponse, Error> {
    let application = application.into_inner();

    // launch web socket actor
    let ws = WsHandler::new(
        application,
        group_id.group_id,
        service_addr.get_ref().clone(),
    );
    let resp = ws::start(ws, &req, stream)?;
    Ok(resp)
}
