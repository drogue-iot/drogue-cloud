use crate::{
    endpoints::{
        params::{DeleteParams, ListParams},
        streamer::ArrayStreamer,
    },
    service::{management::ManagementService, PostgresManagementService},
    WebData,
};
use actix_web::{http::header, web, web::Json, HttpRequest, HttpResponse};
use drogue_client::registry;
use drogue_cloud_registry_events::EventSender;
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_service_api::{auth::user::UserInformation, labels::ParserError};
use drogue_cloud_service_common::error::ServiceError;
use drogue_cloud_service_common::keycloak::KeycloakClient;
use std::convert::TryInto;
use tracing::instrument;

#[instrument(skip(data))]
pub async fn create<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<String>,
    device: Json<registry::v1::Device>,
    user: UserInformation,
    req: HttpRequest,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let app_id = path.into_inner();
    log::debug!("Creating device: '{}' / '{:?}'", app_id, device);

    if device.metadata.name.is_empty()
        || device.metadata.application.is_empty()
        || device.metadata.name.len() > 255
        || std::str::from_utf8(device.metadata.name.as_bytes()).is_err()
    {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let location = req.url_for("device", &[&app_id, &device.metadata.name])?;

    data.service.create_device(&user, device.0).await?;

    let response = HttpResponse::Created()
        .append_header((header::LOCATION, location.as_str()))
        .finish();

    Ok(response)
}

#[instrument(skip(data))]
pub async fn update<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<(String, String)>,
    user: UserInformation,
    device: Json<registry::v1::Device>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let (app_id, device_id) = path.into_inner();

    log::debug!(
        "Updating device: '{}' / '{}' / '{:?}'",
        app_id,
        device_id,
        device
    );

    if app_id.is_empty() || device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }
    if app_id != device.metadata.application || device_id != device.metadata.name {
        return Ok(HttpResponse::BadRequest().finish());
    }

    data.service.update_device(&user, device.0).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[instrument(skip(data))]
pub async fn delete<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<(String, String)>,
    user: UserInformation,
    params: Option<web::Json<DeleteParams>>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let (app, device) = path.into_inner();

    log::debug!("Deleting device: '{}' / '{}'", app, device);

    if app.is_empty() || device.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    data.service
        .delete_device(
            &user,
            &app,
            &device,
            params.map(|p| p.0).unwrap_or_default(),
        )
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

#[instrument(skip(data))]
pub async fn read<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<(String, String)>,
    user: UserInformation,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let (app_id, device_id) = path.into_inner();

    log::debug!("Reading device: '{}' / '{}'", app_id, device_id);

    if app_id.is_empty() || device_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let device = data.service.get_device(&user, &app_id, &device_id).await?;

    let result = match device {
        None => HttpResponse::NotFound().finish(),
        Some(device) => HttpResponse::Ok().json(device),
    };

    Ok(result)
}

#[instrument(skip(data))]
pub async fn list<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<String>,
    params: web::Query<ListParams>,
    user: UserInformation,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let app_id = path.into_inner();

    log::debug!("Listing devices: '{}' ", app_id);

    if app_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let selector = params
        .0
        .labels
        .try_into()
        .map_err(|err: ParserError| ServiceError::InvalidRequest(err.to_string()))?;

    let apps = data
        .service
        .list_devices(user, &app_id, selector, params.0.limit, params.0.offset)
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .streaming(ArrayStreamer::new(apps)))
}
