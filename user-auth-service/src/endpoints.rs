use crate::{
    service::{self, AuthorizationService},
    WebData,
};
use actix_web::{web, HttpResponse};
use drogue_client::user::v1::authz::{AuthorizationRequest, AuthorizationResponse};
use drogue_cloud_service_api::webapp as actix_web;

/// Endpoint to authorize a user operation.
pub async fn authorize(
    req: web::Json<AuthorizationRequest>,
    data: web::Data<WebData<service::PostgresAuthorizationService>>,
) -> Result<HttpResponse, actix_web::Error> {
    let result = match data.service.authorize(req.0).await {
        Ok(outcome) => Ok(HttpResponse::Ok().json(AuthorizationResponse { outcome })),
        Err(e) => Err(e.into()),
    };

    result
}
