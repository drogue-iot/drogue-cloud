use crate::{
    service::{self, ManagementService},
    WebData,
};
use actix_web::{get, web, HttpResponse};
use serde_json::json;

#[get("/health")]
pub async fn health(
    data: web::Data<WebData<service::PostgresManagementService>>,
) -> Result<HttpResponse, actix_web::Error> {
    data.service.is_ready().await?;

    Ok(HttpResponse::Ok().json(json!({"success": true})))
}
