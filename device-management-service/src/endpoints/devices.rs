use crate::{
    service::{ManagementService, PostgresManagementService},
    WebData,
};
use actix_web::{delete, get, http::header, post, put, web, web::Json, HttpResponse};
use drogue_cloud_service_api::management::{Credential, DeviceData};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CreateDevice {
    pub device_id: String,
    pub password: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[post("/{tenant_id}")]
async fn create_device(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(tenant_id): web::Path<String>,
    create: Json<CreateDevice>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Creating device: '{}' / '{:?}'", tenant_id, create);

    let device_id = &create.device_id;

    if tenant_id.is_empty() || device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    // FIXME: we need to allow passing in the full structure
    let device_data = DeviceData {
        credentials: vec![Credential::Password(create.password.clone())],
        properties: create.properties.clone(),
    };

    data.service
        .create_device(&tenant_id, device_id, &device_data)
        .await?;

    let response = HttpResponse::Created()
        // FIXME: create proper URL
        .set_header(header::LOCATION, device_id.clone())
        .finish();

    Ok(response)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct UpdateDevice {
    pub password: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[put("/{tenant_id}/{device_id}")]
async fn update_device(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path((tenant_id, device_id)): web::Path<(String, String)>,
    update: Json<UpdateDevice>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Updating device: '{}' / '{:?}'", tenant_id, update);

    if tenant_id.is_empty() || device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    // FIXME: we need to allow passing in the full structure
    let device_data = DeviceData {
        credentials: vec![Credential::Password(update.password.clone())],
        properties: update.properties.clone(),
    };

    data.service
        .update_device(&tenant_id, &device_id, &device_data)
        .await?;

    let response = HttpResponse::NoContent()
        // FIXME: create proper URL
        .set_header(header::LOCATION, device_id.clone())
        .finish();

    Ok(response)
}

#[delete("/{tenant_id}/{device_id}")]
async fn delete_device(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path((tenant_id, device_id)): web::Path<(String, String)>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Deleting device: '{}' / '{}'", tenant_id, device_id);

    if tenant_id.is_empty() || device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let found = data.service.delete_device(&tenant_id, &device_id).await?;

    let result = match found {
        false => HttpResponse::NotFound().finish(),
        true => HttpResponse::NoContent().finish(),
    };

    Ok(result)
}

#[get("/{tenant_id}/{device_id}")]
async fn read_device(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path((tenant_id, device_id)): web::Path<(String, String)>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Reading device: '{}' / '{}'", tenant_id, device_id);

    if tenant_id.is_empty() || device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let device = data.service.get_device(&tenant_id, &device_id).await?;

    let result = match device {
        None => HttpResponse::NotFound().finish(),
        Some(device) => HttpResponse::Ok().json(device),
    };

    Ok(result)
}
