use crate::{
    service::{self, AuthorizationService},
    WebData,
};
use actix_web::{get, post, web, HttpResponse};
use drogue_cloud_service_api::{
    auth::authz::{AuthorizationRequest, AuthorizationResponse},
    health::HealthCheckedService,
};
use serde_json::json;

#[get("/health")]
pub async fn health(
    data: web::Data<WebData<service::PostgresAuthorizationService>>,
) -> Result<HttpResponse, actix_web::Error> {
    data.service.is_ready().await?;

    Ok(HttpResponse::Ok().json(json!({"success": true})))
}

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
