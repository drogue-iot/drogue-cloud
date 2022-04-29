use crate::service::{CreateResponse, DeviceStateService};
use drogue_cloud_service_api::webapp::{web, *};

pub async fn init(service: web::Data<dyn DeviceStateService>) -> Result<HttpResponse, Error> {
    let response = service.init().await?;
    Ok(HttpResponse::Created().json(response))
}

pub async fn create(
    service: web::Data<dyn DeviceStateService>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, Error> {
    let (instance, id) = path.into_inner();
    Ok(match service.create(instance, id).await? {
        CreateResponse::Created => HttpResponse::Created().finish(),
        CreateResponse::Occupied => HttpResponse::Conflict().finish(),
    })
}

pub async fn delete(
    service: web::Data<dyn DeviceStateService>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, Error> {
    let (instance, id) = path.into_inner();
    service.delete(instance, id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn ping(
    service: web::Data<dyn DeviceStateService>,
    instance: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let response = service.ping(instance.into_inner()).await?;
    Ok(HttpResponse::Ok().json(response))
}
