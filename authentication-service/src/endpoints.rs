use crate::{
    service::{self, AuthenticationService},
    WebData,
};
use actix_web::{post, web, HttpResponse};
use drogue_cloud_service_api::auth::authn::{AuthenticationRequest, AuthenticationResponse};

#[post("/auth")]
pub async fn authenticate(
    req: web::Json<AuthenticationRequest>,
    data: web::Data<WebData<service::PostgresAuthenticationService>>,
) -> Result<HttpResponse, actix_web::Error> {
    let result = match data.service.authenticate(req.0).await {
        Ok(outcome) => Ok(HttpResponse::Ok().json(AuthenticationResponse { outcome })),
        Err(e) => Err(e.into()),
    };

    result
}
