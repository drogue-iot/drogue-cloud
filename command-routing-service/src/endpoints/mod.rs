use crate::service::CommandRoutingService;
use drogue_cloud_service_api::{
    services::command_routing::*,
    webapp::{web, *},
};

pub async fn init(service: web::Data<dyn CommandRoutingService>) -> Result<HttpResponse, Error> {
    let response = service.init().await?;
    Ok(HttpResponse::Created().json(response))
}

pub async fn create(
    service: web::Data<dyn CommandRoutingService>,
    path: web::Path<(String, String, String)>,
    body: web::Json<CreateRequest>,
) -> Result<HttpResponse, Error> {
    let (instance, application, device) = path.into_inner();
    let response = service
        .create(instance, application, device, body.0.token, body.0.state)
        .await?;

    Ok(match response {
        CreateResponse::Created => HttpResponse::Created().finish(),
        CreateResponse::Occupied => HttpResponse::Conflict().finish(),
    })
}

pub async fn delete(
    service: web::Data<dyn CommandRoutingService>,
    path: web::Path<(String, String, String)>,
    body: web::Json<DeleteRequest>,
) -> Result<HttpResponse, Error> {
    let (instance, application, device) = path.into_inner();
    service
        .delete(instance, application, device, body.0.token, body.0.options)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get(
    service: web::Data<dyn CommandRoutingService>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, Error> {
    let (application, device) = path.into_inner();
    Ok(match service.get(application, device).await? {
        Some(state) => HttpResponse::Ok().json(state),
        None => HttpResponse::NotFound().finish(),
    })
}

pub async fn ping(
    service: web::Data<dyn CommandRoutingService>,
    instance: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let response = service.ping(instance.into_inner()).await?;
    Ok(HttpResponse::Ok().json(response))
}
