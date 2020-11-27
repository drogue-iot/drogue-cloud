use actix_web::dev::ServiceRequest;
use actix_web::error::ErrorBadRequest;
use actix_web::http::header;
use actix_web::Error;

use actix_web_httpauth::extractors::basic::{BasicAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;

use actix_web::client::Client;
use actix_web::web::Buf;
use awc::http::StatusCode;
use reqwest::header::{HeaderName, HeaderValue};

const AUTH_SERVICE_URL: &str = "AUTH_SERVICE_URL";
const PROPS_HEADER_NAME: &str = "properties";

pub async fn basic_validator(
    mut req: ServiceRequest,
    cred: BasicAuth,
) -> Result<ServiceRequest, Error> {
    //TODO : get this when initializing the app instead of pulling it each time
    let auth_service_url = std::env::var(AUTH_SERVICE_URL).expect("AUTH_SERVICE_URL must be set");

    let config = req
        .app_data::<Config>()
        .cloned()
        .map(|data| data)
        .unwrap_or_else(Default::default);

    let url = format!("{}", auth_service_url);

    // We fetch the encoded header to avoid re-encoding
    let encoded_basic_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or_else(|| ErrorBadRequest("Missing Authorization header"))?;

    let response = Client::default()
        .get(url)
        .header(header::AUTHORIZATION, encoded_basic_header.clone())
        .send()
        // todo : use a future instead of blocking
        .await;

    match response {
        Ok(mut r) => {
            if r.status() == StatusCode::OK {
                log::debug!("{} authenticated successfully", cred.user_id());
                // todo : use a future instead of blocking
                let props = r.body().await;
                match props {
                    Ok(p) => {
                        req.headers_mut().insert(
                            HeaderName::from_static(PROPS_HEADER_NAME),
                            HeaderValue::from_bytes(p.bytes())
                                .unwrap_or_else(|_| HeaderValue::from_static("{}")),
                        );
                        Ok(req)
                    }
                    Err(_) => Ok(req),
                }
            } else {
                log::debug!(
                    "Authentication failed for {}. Result: {}",
                    cred.user_id(),
                    r.status()
                );
                Err(AuthenticationError::from(config).into())
            }
        }
        Err(e) => {
            log::warn!("Error while authenticating {}. {}", cred.user_id(), e);
            Err(AuthenticationError::from(config).into())
        }
    }
}
