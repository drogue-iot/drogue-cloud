use crate::{
    service::{self, AuthenticationService},
    WebData,
};
use actix_web::{web, HttpResponse};
use drogue_cloud_service_api::auth::device::authn::{
    AuthenticationRequest, AuthenticationResponse, AuthorizeGatewayRequest,
    AuthorizeGatewayResponse,
};
use drogue_cloud_service_api::webapp as actix_web;
use tracing::instrument;

#[instrument(skip(data))]
pub async fn authenticate(
    req: web::Json<AuthenticationRequest>,
    data: web::Data<WebData<service::PostgresAuthenticationService>>,
) -> Result<HttpResponse, actix_web::Error> {
    let result = match data.service.authenticate(req.0).await {
        Ok(outcome) => Ok(HttpResponse::Ok().json(AuthenticationResponse { outcome })),
        Err(e) => Err(e.into()),
    };

    result
}

#[instrument(skip(data))]
pub async fn authorize_as(
    req: web::Json<AuthorizeGatewayRequest>,
    data: web::Data<WebData<service::PostgresAuthenticationService>>,
) -> Result<HttpResponse, actix_web::Error> {
    let result = match data.service.authorize_gateway(req.0).await {
        Ok(outcome) => Ok(HttpResponse::Ok().json(AuthorizeGatewayResponse { outcome })),
        Err(e) => Err(e.into()),
    };

    result
}
