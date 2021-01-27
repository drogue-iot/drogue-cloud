use crate::{
    service::{ManagementService, PostgresManagementService},
    WebData,
};
use actix_web::{http::header, web, web::Json, HttpRequest, HttpResponse};
use drogue_cloud_service_api::management::TenantData;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateTenant {
    pub tenant_id: String,
    #[serde(default)]
    pub disabled: bool,
}

pub async fn create(
    data: web::Data<WebData<PostgresManagementService>>,
    create: Json<CreateTenant>,
    req: HttpRequest,
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

    let location = req.url_for("tenant", &[tenant_id])?;

    let response = HttpResponse::Created()
        .set_header(header::LOCATION, location.into_string())
        .finish();

    Ok(response)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateTenant {
    #[serde(default)]
    pub disabled: bool,
}

pub async fn update(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(tenant_id): web::Path<String>,
    update: Json<UpdateTenant>,
    req: HttpRequest,
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

    let location = req.url_for("tenant", &[tenant_id])?;

    let response = HttpResponse::NoContent()
        .set_header(header::LOCATION, location.into_string())
        .finish();

    Ok(response)
}

pub async fn delete(
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

pub async fn read(
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
