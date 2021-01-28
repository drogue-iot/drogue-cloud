use crate::{
    service::{ManagementService, PostgresManagementService},
    WebData,
};
use actix_web::{http::header, web, web::Json, HttpRequest, HttpResponse};
use drogue_cloud_service_api::management::Application;

pub async fn create(
    data: web::Data<WebData<PostgresManagementService>>,
    app: Json<Application>,
    req: HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Creating application: '{:?}'", app);

    if app.metadata.name.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let location = req.url_for("app", &[&app.metadata.name])?;

    data.service.create_app(app.0).await?;

    let response = HttpResponse::Created()
        .set_header(header::LOCATION, location.into_string())
        .finish();

    Ok(response)
}

pub async fn update(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(app_id): web::Path<String>,
    app: Json<Application>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Updating app: '{:?}'", app);

    if app_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    if app_id != app.metadata.name {
        return Ok(HttpResponse::BadRequest().finish());
    }

    data.service.update_app(app.0).await?;

    let response = HttpResponse::NoContent().finish();

    Ok(response)
}

pub async fn delete(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(app_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Deleting app: '{}'", app_id);

    if app_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let found = data.service.delete_app(&app_id).await?;

    let result = match found {
        false => HttpResponse::NotFound().finish(),
        true => HttpResponse::NoContent().finish(),
    };

    Ok(result)
}

pub async fn read(
    data: web::Data<WebData<PostgresManagementService>>,
    web::Path(app_id): web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    log::debug!("Reading app: '{}'", app_id);

    if app_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let app = data.service.get_app(&app_id).await?;

    Ok(match app {
        None => HttpResponse::NotFound().finish(),
        Some(app) => HttpResponse::Ok().json(app),
    })
}
