use crate::service::AccessTokenService;
use actix_web::{web, HttpResponse};
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_service_api::{
    auth::user::{
        authn::{AuthenticationRequest, AuthenticationResponse, Outcome},
        UserInformation,
    },
    token::AccessTokenCreationOptions,
};
use std::ops::Deref;

pub struct WebData<S: AccessTokenService> {
    pub service: S,
}

impl<S: AccessTokenService> Deref for WebData<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

pub async fn create<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
    opts: web::Query<AccessTokenCreationOptions>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AccessTokenService + 'static,
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
    S: AccessTokenService + 'static,
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
    S: AccessTokenService + 'static,
{
    let result = match service.delete(&user, prefix.into_inner()).await {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => Err(e.into()),
    };

    result
}

/// Endpoint to authenticate a user token
pub async fn authenticate<S>(
    req: web::Json<AuthenticationRequest>,
    service: web::Data<WebData<S>>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AccessTokenService + 'static,
{
    let result = match service.authenticate(&req.user_id, &req.access_token).await {
        Ok(Some(details)) => Ok(HttpResponse::Ok().json(AuthenticationResponse {
            outcome: Outcome::Known(details),
        })),
        Ok(None) => Ok(HttpResponse::Ok().json(AuthenticationResponse {
            outcome: Outcome::Unknown,
        })),
        Err(e) => Err(e.into()),
    };

    result
}
