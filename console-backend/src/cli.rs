use crate::auth::OpenIdClient;
use actix_web::{get, http, web, HttpRequest, HttpResponse, Responder};
use actix_web_httpauth::headers::authorization::{Basic, Scheme};
use http::header;
use serde_json::json;

#[get("/cli/login")]
pub async fn login(
    login_handler: Option<web::Data<OpenIdClient>>,
    req: HttpRequest,
) -> impl Responder {
    if let Some(client) = login_handler {
        let basic = match req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| Basic::parse(v).ok())
        {
            Some(user) => user,
            None => return HttpResponse::Unauthorized().finish(),
        };

        let username = basic.user_id();
        let password = match basic.password() {
            Some(password) => password,
            None => return HttpResponse::Unauthorized().finish(),
        };

        let result = client
            .client
            .request_token_using_password_credentials(
                username,
                password,
                Some(&format!("{} offline_access", &client.scopes)),
            )
            .await;

        match result {
            Ok(token) => {
                log::debug!("Token: {:?}", token);

                HttpResponse::Found().json(json!({
                    "access": token.access_token,
                    "id": token.id_token,
                    "refresh": token.refresh_token
                }))
            }
            Err(err) => {
                log::info!("Error: {:?}", err);
                HttpResponse::Unauthorized().finish()
            }
        }
    } else {
        // if we are missing the authenticator, we hide ourselves
        HttpResponse::NotFound().finish()
    }
}
