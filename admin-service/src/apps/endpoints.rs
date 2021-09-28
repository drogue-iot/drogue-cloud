use crate::apps::service::AdminService;
use actix_web::{web, HttpResponse};
use drogue_cloud_service_api::{
    admin::{Members, TransferOwnership},
    auth::user::UserInformation,
};

use std::ops::Deref;

pub struct WebData<S: AdminService> {
    pub service: S,
}

impl<S: AdminService> Deref for WebData<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

/// Initiate an ownership transfer
pub async fn transfer<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
    app_id: web::Path<String>,
    payload: web::Json<TransferOwnership>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AdminService + 'static,
{
    let result = match service
        .transfer(&user, app_id.into_inner(), payload.0)
        .await
    {
        Ok(key) => Ok(HttpResponse::Accepted().json(key)),
        Err(e) => Err(e.into()),
    };

    result
}

/// Cancel an ownership transfer
pub async fn cancel<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
    app_id: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AdminService + 'static,
{
    let result = match service.cancel(&user, app_id.into_inner()).await {
        Ok(key) => Ok(HttpResponse::NoContent().json(key)),
        Err(e) => Err(e.into()),
    };

    result
}

/// Accept an ownership transfer
pub async fn accept<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
    app_id: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AdminService + 'static,
{
    let result = match service.accept(&user, app_id.into_inner()).await {
        Ok(key) => Ok(HttpResponse::NoContent().json(key)),
        Err(e) => Err(e.into()),
    };

    result
}

/// Get member list
pub async fn get_members<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
    app_id: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AdminService + 'static,
{
    let result = match service.get_members(&user, app_id.into_inner()).await {
        Ok(members) => Ok(HttpResponse::Ok().json(members)),
        Err(e) => Err(e.into()),
    };

    result
}

/// Set member list
pub async fn set_members<S>(
    user: UserInformation,
    service: web::Data<WebData<S>>,
    app_id: web::Path<String>,
    payload: web::Json<Members>,
) -> Result<HttpResponse, actix_web::Error>
where
    S: AdminService + 'static,
{
    let result = match service
        .set_members(&user, app_id.into_inner(), payload.0)
        .await
    {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(e) => Err(e.into()),
    };

    result
}
