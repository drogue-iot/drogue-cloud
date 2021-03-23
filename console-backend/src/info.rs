use actix_web::{get, web, HttpResponse, Responder};

use drogue_cloud_service_common::endpoints::EndpointSourceType;
use serde_json::json;

#[get("/info")]
pub async fn get_info(endpoint_source: web::Data<EndpointSourceType>) -> impl Responder {
    match endpoint_source.eval_endpoints().await {
        Ok(endpoints) => HttpResponse::Ok().json(endpoints),
        Err(err) => HttpResponse::InternalServerError().json(json!( {"error": err.to_string()})),
    }
}

#[get("/endpoints")]
pub async fn get_public_endpoints(
    endpoint_source: web::Data<EndpointSourceType>,
) -> impl Responder {
    match endpoint_source.eval_endpoints().await {
        Ok(endpoints) => HttpResponse::Ok().json(endpoints.publicize()),
        Err(err) => HttpResponse::InternalServerError().json(json!( {"error": err.to_string()})),
    }
}
