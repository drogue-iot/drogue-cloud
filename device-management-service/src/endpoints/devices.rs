use crate::{
    service::{ManagementService, PostgresManagementService},
    WebData,
};
use actix_web::{http::header, web, web::Json, HttpRequest, HttpResponse};
use drogue_cloud_service_api::management::{Credential, DeviceData};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateDevice {
    pub device_id: String,
    pub password: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

pub async fn create(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(tenant_id): web::Path<String>,
    create: Json<CreateDevice>,
    req: HttpRequest,
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

    let location = req.url_for("device", &[&tenant_id, &device_id])?;

    let response = HttpResponse::Created()
        .set_header(header::LOCATION, location.into_string())
        .finish();

    Ok(response)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateDevice {
    pub password: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

pub async fn update(
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

pub async fn delete(
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

pub async fn read(
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
