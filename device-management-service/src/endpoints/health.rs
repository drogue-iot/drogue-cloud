use crate::{
    service::{self},
    WebData,
};
use actix_web::{web, HttpResponse};
use drogue_cloud_database_common::DatabaseService;
use drogue_cloud_registry_events::EventSender;
use serde_json::json;

pub async fn health<S>(
    data: web::Data<WebData<service::PostgresManagementService<S>>>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
{
    data.service.is_ready().await?;

    Ok(HttpResponse::Ok().json(json!({"success": true})))
}
