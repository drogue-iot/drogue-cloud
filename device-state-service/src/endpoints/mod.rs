use crate::service::DeviceStateService;
use drogue_cloud_service_api::{
    services::device_state::*,
    webapp::{web, *},
};

pub async fn init(service: web::Data<dyn DeviceStateService>) -> Result<HttpResponse, Error> {
    let response = service.init().await?;
    Ok(HttpResponse::Created().json(response))
}

pub async fn create(
    service: web::Data<dyn DeviceStateService>,
    path: web::Path<(String, String, String)>,
    body: web::Json<DeviceState>,
) -> Result<HttpResponse, Error> {
    let (instance, application, device) = path.into_inner();
    Ok(
        match service
            .create(instance, application, device, body.0)
            .await?
        {
            CreateResponse::Created => HttpResponse::Created().finish(),
            CreateResponse::Occupied => HttpResponse::Conflict().finish(),
        },
    )
}

pub async fn delete(
    service: web::Data<dyn DeviceStateService>,
    path: web::Path<(String, String, String)>,
) -> Result<HttpResponse, Error> {
    let (instance, application, device) = path.into_inner();
    service.delete(instance, application, device).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get(
    service: web::Data<dyn DeviceStateService>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, Error> {
    let (application, device) = path.into_inner();
    Ok(match service.get(application, device).await? {
        Some(state) => HttpResponse::Ok().json(state),
        None => HttpResponse::NotFound().finish(),
    })
}

pub async fn ping(
    service: web::Data<dyn DeviceStateService>,
    instance: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let response = service.ping(instance.into_inner()).await?;
    Ok(HttpResponse::Ok().json(response))
}
