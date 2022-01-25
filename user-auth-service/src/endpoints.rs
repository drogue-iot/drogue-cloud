use crate::{
    service::{self, AuthorizationService},
    WebData,
};
use actix_web::{post, web, HttpResponse};
use drogue_cloud_service_api::auth::user::authz::{AuthorizationRequest, AuthorizationResponse};
use drogue_cloud_service_api::webapp as actix_web;

#[post("/authz")]
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
