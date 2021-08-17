use crate::demos::get_demos;
use actix_web::{get, web, HttpResponse, Responder};
use drogue_cloud_console_common::EndpointInformation;
use drogue_cloud_service_api::{endpoints::Endpoints, version::DrogueVersion};
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Api;

pub async fn get_info(
    endpoints: web::Data<Endpoints>,
    config_maps: web::Data<Api<ConfigMap>>,
) -> impl Responder {
    let info = EndpointInformation {
        endpoints: endpoints.get_ref().clone(),
        demos: get_demos(&config_maps).await.unwrap_or_default(),
    };

    HttpResponse::Ok().json(info)
}

#[get("/drogue-endpoints")]
pub async fn get_public_endpoints(endpoints: web::Data<Endpoints>) -> impl Responder {
    HttpResponse::Ok().json(endpoints.publicize())
}

#[get("/drogue-version")]
pub async fn get_drogue_version() -> impl Responder {
    HttpResponse::Ok().json(DrogueVersion::new())
}
