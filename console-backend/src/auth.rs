use actix_web::{get, http, web, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use drogue_cloud_console_common::UserInfo;
use drogue_cloud_service_common::error::ErrorResponse;
use openid::{biscuit::jws::Compact, Bearer, Configurable, StandardClaims, Token};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Clone)]
pub struct OpenIdClient {
    pub client: openid::Client,
    pub scopes: String,
    pub account_url: Option<String>,
}

#[get("/ui/login")]
pub async fn login(login_handler: Option<web::Data<OpenIdClient>>) -> impl Responder {
    if let Some(client) = login_handler {
        let auth_url = client.client.auth_uri(Some(&client.scopes), None);

        HttpResponse::Found()
            .append_header((http::header::LOCATION, auth_url.to_string()))
            .finish()
    } else {
        // if we are missing the authenticator, we hide ourselves
        HttpResponse::NotFound().finish()
    }
}

/// An endpoint that will redirect to the SSO "end session" endpoint
#[get("/ui/logout")]
pub async fn logout(login_handler: Option<web::Data<OpenIdClient>>) -> impl Responder {
    if let Some(client) = login_handler {
        if let Some(url) = &client.client.provider.config().end_session_endpoint {
            let mut url = url.clone();

            if let Some(redirect) = &client.client.redirect_uri {
                url.query_pairs_mut().append_pair("redirect_uri", redirect);
            }

            return HttpResponse::Found()
                .append_header((http::header::LOCATION, url.to_string()))
                .finish();
        } else {
            log::info!("Missing logout URL");
        }
    }

    // if we are missing the authenticator, we hide ourselves
    HttpResponse::NotFound().finish()
}

#[derive(Deserialize, Debug)]
pub struct LoginQuery {
    code: String,
    nonce: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TokenResponse {
    pub bearer: Bearer,
    pub expires: Option<DateTime<Utc>>,
    pub userinfo: Option<UserInfo>,
}

#[get("/ui/token")]
pub async fn code(
    login_handler: Option<web::Data<OpenIdClient>>,
    query: web::Query<LoginQuery>,
) -> impl Responder {
    if let Some(client) = login_handler {
        let response = client
            .client
            .authenticate(&query.code, query.nonce.as_deref(), None)
            .await;

        log::info!(
            "Response: {:?}",
            response.as_ref().map(|r| r.bearer.clone())
        );

        match response {
            Ok(token) => {
                let userinfo = make_userinfo(&client, &token);

                let expires = token.bearer.expires;
                HttpResponse::Ok().json(TokenResponse {
                    bearer: token.bearer,
                    expires,
                    userinfo,
                })
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

fn make_userinfo(client: &OpenIdClient, token: &Token<StandardClaims>) -> Option<UserInfo> {
    token.id_token.as_ref().and_then(|t| match t {
        Compact::Decoded { payload, .. } => {
            log::debug!("Userinfo: {:#?}", payload.userinfo);
            Some(UserInfo {
                id: payload.sub.clone(),
                name: payload
                    .userinfo
                    .preferred_username
                    .as_ref()
                    .unwrap_or(&payload.sub)
                    .clone(),
                full_name: payload.userinfo.name.clone(),
                account_url: client.account_url.clone(),
                email_verified: payload.userinfo.email_verified,
                email: payload.userinfo.email.clone(),
            })
        }
        Compact::Encoded(_) => None,
    })
}

#[derive(Deserialize, Debug)]
pub struct RefreshQuery {
    refresh_token: String,
}

#[get("/ui/refresh")]
pub async fn refresh(
    login_handler: Option<web::Data<OpenIdClient>>,
    query: web::Query<RefreshQuery>,
) -> impl Responder {
    if let Some(client) = login_handler {
        let response = client
            .client
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
                let expires = bearer.expires;
                HttpResponse::Ok().json(TokenResponse {
                    bearer,
                    expires,
                    userinfo: None,
                })
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
