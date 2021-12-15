use crate::demos::get_demos;
use actix_web::{get, web, HttpResponse, Responder};
use drogue_cloud_console_common::EndpointInformation;
use drogue_cloud_service_api::{endpoints::Endpoints, version::DrogueVersion};
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Api;

#[derive(Clone)]
pub enum DemoFetcher {
    None,
    Kube(Api<ConfigMap>),
}

impl DemoFetcher {
    async fn get_demos(&self) -> Vec<(String, String)> {
        match self {
            DemoFetcher::None => Vec::new(),
            DemoFetcher::Kube(config_maps) => get_demos(&config_maps).await.unwrap_or_default(),
        }
    }
}

pub async fn get_info(
    endpoints: web::Data<Endpoints>,
    demos: web::Data<DemoFetcher>,
) -> impl Responder {
    let info = EndpointInformation {
        endpoints: endpoints.get_ref().clone(),
        demos: demos.get_ref().get_demos().await,
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
