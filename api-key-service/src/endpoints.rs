use crate::{data::ApiKeyCreationOptions, service::ApiKeyService};
use actix_web::{web, HttpResponse};
use drogue_cloud_service_api::auth::user::authn::{
    AuthenticationRequest, AuthenticationResponse, Outcome,
};
use drogue_cloud_service_common::auth::UserInformation;
use std::ops::Deref;

pub struct WebData<S: ApiKeyService> {
    pub service: S,
}

impl<S: ApiKeyService> Deref for WebData<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

pub async fn create<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
    opts: web::Query<ApiKeyCreationOptions>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: ApiKeyService + 'static,
{
    let result = match service.create(&user, opts.0).await {
        Ok(key) => Ok(HttpResponse::Ok().json(key)),
        Err(e) => Err(e.into()),
    };

    result
}

pub async fn list<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: ApiKeyService + 'static,
{
    let result = match service.list(&user).await {
        Ok(outcome) => Ok(HttpResponse::Ok().json(outcome)),
        Err(e) => Err(e.into()),
    };

    result
}

pub async fn delete<S>(
    prefix: web::Path<String>,
    user: UserInformation,
    service: web::Data<WebData<S>>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: ApiKeyService + 'static,
{
    let result = match service.delete(&user, prefix.into_inner()).await {
        Ok(outcome) => Ok(HttpResponse::Ok().json(outcome)),
        Err(e) => Err(e.into()),
    };

    result
}

/// Endpoint to authenticate a user key
pub async fn authenticate<S>(
    req: web::Json<AuthenticationRequest>,
    service: web::Data<WebData<S>>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: ApiKeyService + 'static,
{
    let result = match service.authenticate(&req.user_id, &req.api_key).await {
        Ok(true) => Ok(HttpResponse::Ok().json(AuthenticationResponse {
            outcome: Outcome::Known,
        })),
        Ok(false) => Ok(HttpResponse::Ok().json(AuthenticationResponse {
            outcome: Outcome::Unknown,
        })),
        Err(e) => Err(e.into()),
    };

    result
}
