use crate::WebData;
use actix_web::{get, web, HttpResponse};
use drogue_cloud_database_common::DatabaseService;
use serde_json::json;

#[get("/health")]
pub async fn health(data: web::Data<WebData>) -> Result<HttpResponse, actix_web::Error> {
    data.service.is_ready().await?;

    Ok(HttpResponse::Ok().json(json!({"success": true})))
}
