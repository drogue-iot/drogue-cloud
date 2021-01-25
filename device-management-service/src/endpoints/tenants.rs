use crate::{
    service::{ManagementService, PostgresManagementService},
    WebData,
};
use actix_web::{delete, get, http::header, post, put, web, web::Json, HttpResponse};
use drogue_cloud_service_api::management::TenantData;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CreateTenant {
    pub tenant_id: String,
    #[serde(default)]
    pub disabled: bool,
}

#[post("")]
async fn create_tenant(
    data: web::Data<WebData<PostgresManagementService>>,
    create: Json<CreateTenant>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Creating tenant: '{:?}'", create);

    let tenant_id = &create.tenant_id;

    if tenant_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    // FIXME: we need to allow passing in the full structure
    let tenant_data = TenantData {
        disabled: create.disabled,
    };

    data.service.create_tenant(&tenant_id, &tenant_data).await?;

    let response = HttpResponse::Created()
        // FIXME: create proper URL
        .set_header(header::LOCATION, tenant_id.clone())
        .finish();

    Ok(response)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct UpdateTenant {
    #[serde(default)]
    pub disabled: bool,
}

#[put("/{tenant_id}")]
async fn update_tenant(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(tenant_id): web::Path<String>,
    update: Json<UpdateTenant>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Updating tenant: '{}' / '{:?}'", tenant_id, update);

    if tenant_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    // FIXME: we need to allow passing in the full structure
    let tenant_data = TenantData {
        disabled: update.disabled,
    };

    data.service.update_tenant(&tenant_id, &tenant_data).await?;

    let response = HttpResponse::NoContent()
        // FIXME: create proper URL
        .set_header(header::LOCATION, tenant_id.clone())
        .finish();

    Ok(response)
}

#[delete("/{tenant_id}")]
async fn delete_tenant(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(tenant_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Deleting tenant: '{}'", tenant_id);

    if tenant_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let found = data.service.delete_tenant(&tenant_id).await?;

    let result = match found {
        false => HttpResponse::NotFound().finish(),
        true => HttpResponse::NoContent().finish(),
    };

    Ok(result)
}

#[get("/{tenant_id}")]
async fn read_tenant(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(tenant_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Reading tenant: '{}'", tenant_id);

    if tenant_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let device = data.service.get_tenant(&tenant_id).await?;

    let result = match device {
        None => HttpResponse::NotFound().finish(),
        Some(device) => HttpResponse::Ok().json(device),
    };

    Ok(result)
}
