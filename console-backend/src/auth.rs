use drogue_cloud_service_common::error::ErrorResponse;
use drogue_cloud_service_common::openid::Authenticator;

use actix_web::{get, http, web, HttpResponse, Responder};
use drogue_cloud_console_common::UserInfo;
use openid::{biscuit::jws::Compact, Bearer};
use serde::Deserialize;
use serde_json::json;
use std::fmt::Debug;

#[get("/ui/login")]
pub async fn login(login_handler: web::Data<Authenticator>) -> impl Responder {
    if let Some(client) = login_handler.client.as_ref() {
        let auth_url = client.auth_uri(Some(&login_handler.scopes), None);

        HttpResponse::Found()
            .header(http::header::LOCATION, auth_url.to_string())
            .finish()
    } else {
        // if we are missing the authenticator, we hide ourselves
        HttpResponse::NotFound().finish()
    }
}

#[derive(Deserialize, Debug)]
pub struct LoginQuery {
    code: String,
    nonce: Option<String>,
}

#[get("/ui/token")]
pub async fn code(
    login_handler: web::Data<Authenticator>,
    query: web::Query<LoginQuery>,
) -> impl Responder {
    if let Some(client) = login_handler.client.as_ref() {
        let response = client
            .authenticate(&query.code, query.nonce.as_deref(), None)
            .await;

        log::info!(
            "Response: {:?}",
            response.as_ref().map(|r| r.bearer.clone())
        );

        match response {
            Ok(token) => {
                let userinfo = token.id_token.and_then(|t| match t {
                    Compact::Decoded { payload, .. } => Some(UserInfo {
                        email_verified: payload.userinfo.email_verified,
                        email: payload.userinfo.email,
                    }),
                    Compact::Encoded(_) => None,
                });

                HttpResponse::Ok()
                    .json(json!({ "bearer": token.bearer, "expires": token.bearer.expires, "userinfo": userinfo}))
            }
            Err(err) => HttpResponse::Unauthorized().json(ErrorResponse {
                error: "Unauthorized".to_string(),
                message: format!("Code invalid: {:?}", err),
            }),
        }
    } else {
        // if we are missing the authenticator, we hide ourselves
        HttpResponse::NotFound().finish()
    }
}

#[derive(Deserialize, Debug)]
pub struct RefreshQuery {
    refresh_token: String,
}

#[get("/ui/refresh")]
pub async fn refresh(
    login_handler: web::Data<Authenticator>,
    query: web::Query<RefreshQuery>,
) -> impl Responder {
    if let Some(client) = login_handler.client.as_ref() {
        let response = client
            .refresh_token(
                Bearer {
                    refresh_token: Some(query.0.refresh_token),
                    access_token: String::new(),
                    expires: None,
                    id_token: None,
                    scope: None,
                },
                None,
            )
            .await;

        log::info!("Response: {:?}", response.as_ref());

        match response {
            Ok(bearer) => {
                HttpResponse::Ok().json(json!({ "bearer": bearer, "expires": bearer.expires, }))
            }
            Err(err) => HttpResponse::Unauthorized().json(ErrorResponse {
                error: "Unauthorized".to_string(),
                message: format!("Refresh token invalid: {:?}", err),
            }),
        }
    } else {
        // if we are missing the authenticator, we hide ourselves
        HttpResponse::NotFound().finish()
    }
}
