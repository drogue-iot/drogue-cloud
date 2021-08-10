use crate::auth::{Credentials, UsernameAndApiKey};
use crate::service::Service;
use crate::wshandler::WsHandler;

use actix::Addr;
use actix_web::web::Payload;
use actix_web::{get, web, Either, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::extractors::bearer::BearerAuth;
use std::sync::Arc;

use drogue_cloud_service_common::client::UserAuthClient;
use drogue_cloud_service_common::error::ServiceError;
use drogue_cloud_service_common::openid::Authenticator;

#[get("/{application}")]
pub async fn start_connection(
    req: HttpRequest,
    stream: Payload,
    auth: web::Either<BearerAuth, BasicAuth>,
    auth_client: web::Data<Option<Authenticator>>,
    authz_client: web::Data<Option<Arc<UserAuthClient>>>,
    authorize_api_keys: web::Data<bool>,
    application: web::Path<String>,
    service_addr: web::Data<Addr<Service>>,
) -> Result<HttpResponse, Error> {
    let application = application.into_inner();

    let auth_client = auth_client.get_ref().clone();
    let authz_client = authz_client.get_ref().clone();

    match (auth_client, authz_client) {
        (Some(auth_client), Some(authz_client)) => {
            let credentials = match auth {
                Either::Left(bearer) => Ok(Credentials::Token(bearer.token().to_string())),
                Either::Right(basic) => {
                    if authorize_api_keys.get_ref().clone() {
                        Ok(Credentials::ApiKey(UsernameAndApiKey {
                            username: basic.user_id().to_string(),
                            key: basic.password().map(|k| k.to_string()),
                        }))
                    } else {
                        log::debug!("API keys authentication disabled");
                        Err(ServiceError::InvalidRequest(
                            "API keys authentication disabled".to_string(),
                        ))
                    }
                }
            }?;

            // authentication
            credentials
                .authenticate_and_authorize(application.clone(), &authz_client, auth_client)
                .await
                .or(Err(ServiceError::AuthenticationError))?;
        }
        // authentication disabled
        _ => {}
    }

    // launch web socket actor
    let ws = WsHandler::new(application, service_addr.get_ref().clone());
    let resp = ws::start(ws, &req, stream)?;
    Ok(resp)
}
