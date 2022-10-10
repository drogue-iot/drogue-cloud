use crate::{service::Service, wshandler::WsHandler};
use actix::Addr;
use actix_web::{
    web::{self, Payload},
    Error, HttpRequest, HttpResponse,
};
use actix_web_actors::ws;
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_service_common::actix_auth::authentication::AuthenticatedUntil;
use drogue_cloud_service_common::client::UserAuthClient;
use drogue_cloud_service_common::openid::Authenticator;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct GroupId {
    group_id: Option<String>,
}

pub async fn start_connection(
    req: HttpRequest,
    stream: Payload,
    application: web::Path<String>,
    service_addr: web::Data<Addr<Service>>,
    web::Query(group_id): web::Query<GroupId>,
    auth_expiration: Option<web::ReqData<AuthenticatedUntil>>,
) -> Result<HttpResponse, Error> {
    let application = application.into_inner();

    let authenticator = req.app_data().cloned();
    let user_auth = req.app_data().cloned();

    log::debug!(
        "Auth state - authenticator: {}, userAuth: {}",
        authenticator.is_some(),
        user_auth.is_some()
    );

    start_websocket(
        req,
        stream,
        application,
        None,
        service_addr,
        group_id.group_id,
        auth_expiration,
        authenticator,
        user_auth,
    )
}

pub async fn start_connection_with_channel_filter(
    req: HttpRequest,
    stream: Payload,
    params: web::Path<(String, String)>,
    service_addr: web::Data<Addr<Service>>,
    web::Query(group_id): web::Query<GroupId>,
    auth_expiration: Option<web::ReqData<AuthenticatedUntil>>,
) -> Result<HttpResponse, Error> {
    let (application, channel) = params.into_inner();

    let authenticator = req.app_data().cloned();
    let user_auth = req.app_data().cloned();

    log::debug!(
        "Auth state - authenticator: {}, userAuth: {}",
        authenticator.is_some(),
        user_auth.is_some()
    );

    start_websocket(
        req,
        stream,
        application,
        Some(channel),
        service_addr,
        group_id.group_id,
        auth_expiration,
        authenticator,
        user_auth,
    )
}

fn start_websocket(
    req: HttpRequest,
    stream: Payload,
    application: String,
    channel: Option<String>,
    service_addr: web::Data<Addr<Service>>,
    group_id: Option<String>,
    auth_expiration: Option<web::ReqData<AuthenticatedUntil>>,
    authenticator: Option<Authenticator>,
    user_auth: Option<UserAuthClient>,
) -> Result<HttpResponse, Error> {
    let auth_expiration = auth_expiration.map(|e| e.into_inner().0);

    // launch web socket actor
    let ws = WsHandler::new(
        application,
        group_id,
        channel,
        service_addr.get_ref().clone(),
        auth_expiration,
        authenticator,
        user_auth,
    );

    ws::start(ws, &req, stream)
}
