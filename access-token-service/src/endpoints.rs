use crate::service::AccessTokenService;
use drogue_client::registry::v1::Client;
use drogue_client::user::v1::authn::{AuthenticationRequest, AuthenticationResponse, Outcome};
use drogue_cloud_service_api::{
    auth::user::UserInformation,
    token::AccessTokenCreationOptions,
    webapp::{self as actix_web, web, HttpResponse},
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
    registry: web::Data<Client>,
    opts: web::Json<AccessTokenCreationOptions>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AccessTokenService + 'static,
{
    let result = match service.create(&user, opts.0, &registry.into_inner()).await {
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
