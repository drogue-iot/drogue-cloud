use super::streamer::ArrayStreamer;
use crate::{
    endpoints::params::{DeleteParams, ListParams},
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
use hostname_validator;
use std::convert::TryInto;
use tracing::instrument;

#[instrument(skip(data))]
pub async fn create<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    app: Json<registry::v1::Application>,
    user: UserInformation,
    req: HttpRequest,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    log::debug!("Creating application: '{:?}'", app);

    if app.metadata.name.is_empty() || !hostname_validator::is_valid(app.metadata.name.as_str()) {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let location = req.url_for("app", &[&app.metadata.name])?;

    data.service.create_app(&user, app.0).await?;

    let response = HttpResponse::Created()
        .append_header((header::LOCATION, String::from(location)))
        .finish();

    Ok(response)
}

#[instrument(skip(data))]
pub async fn update<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<String>,
    app: Json<registry::v1::Application>,
    user: UserInformation,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let app_id = path.into_inner();

    log::debug!("Updating app: '{:?}'", app);

    if app_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    if app_id != app.metadata.name {
        return Ok(HttpResponse::BadRequest().finish());
    }

    data.service.update_app(&user, app.0).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[instrument(skip(data))]
pub async fn delete<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<String>,
    params: Option<web::Json<DeleteParams>>,
    user: UserInformation,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let app = path.into_inner();

    log::debug!("Deleting app: '{}'", app);

    if app.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    data.service
        .delete_app(&user, &app, params.map(|p| p.0).unwrap_or_default())
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

#[instrument(skip(data))]
pub async fn read<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    path: web::Path<String>,
    user: UserInformation,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    let app_id = path.into_inner();
    log::debug!("Reading app: '{}'", app_id);

    if app_id.is_empty() {
        return Ok(HttpResponse::BadRequest().finish());
    }

    let app = data.service.get_app(&user, &app_id).await?;

    Ok(match app {
        None => HttpResponse::NotFound().finish(),
        Some(app) => HttpResponse::Ok().json(app),
    })
}

#[instrument(skip(data))]
pub async fn list<S, K>(
    data: web::Data<WebData<PostgresManagementService<S, K>>>,
    params: web::Query<ListParams>,
    user: UserInformation,
) -> Result<HttpResponse, actix_web::Error>
where
    S: EventSender + Clone,
    K: KeycloakClient + Send + Sync,
{
    log::debug!("Listing apps");

    let selector = params
        .0
        .labels
        .try_into()
        .map_err(|err: ParserError| ServiceError::InvalidRequest(err.to_string()))?;

    let apps = data
        .service
        .list_apps(user, selector, params.0.limit, params.0.offset)
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .streaming(ArrayStreamer::new(apps)))
}
