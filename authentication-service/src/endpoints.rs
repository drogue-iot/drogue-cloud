use crate::{
    service::{self, AuthenticationService},
    WebData,
};
use actix_web::{get, post, web, HttpResponse};
use drogue_cloud_service_api::{
    auth::{AuthenticationRequest, AuthenticationResponse},
    health::HealthCheckedService,
};
use serde_json::json;

#[get("/health")]
pub async fn health(
    data: web::Data<WebData<service::PostgresAuthenticationService>>,
) -> Result<HttpResponse, actix_web::Error> {
    data.service.is_ready().await?;

    Ok(HttpResponse::Ok().json(json!({"success": true})))
}

#[post("/auth")]
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
