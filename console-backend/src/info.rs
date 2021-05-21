use actix_web::{get, web, HttpResponse, Responder};
use drogue_cloud_service_api::{endpoints::Endpoints, version::DrogueVersion};

#[get("/info")]
pub async fn get_info(endpoints: web::Data<Endpoints>) -> impl Responder {
    HttpResponse::Ok().json(endpoints)
}

#[get("/drogue-endpoints")]
pub async fn get_public_endpoints(endpoints: web::Data<Endpoints>) -> impl Responder {
    HttpResponse::Ok().json(endpoints.publicize())
}

#[get("/drogue-version")]
pub async fn get_drogue_version() -> impl Responder {
    HttpResponse::Ok().json(DrogueVersion::new())
}
