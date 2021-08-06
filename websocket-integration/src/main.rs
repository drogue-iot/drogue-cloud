mod auth;
mod messages;
mod service;
mod wshandler;

use crate::wshandler::WsHandler;

use dotenv::dotenv;

use actix_web::web::Payload;
use actix_web::{get, web, App, Either, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::extractors::bearer::BearerAuth;

use drogue_cloud_service_common::{
    config::ConfigFromEnv, health::HealthServer, openid::Authenticator,
};
use drogue_cloud_service_common::{defaults, health::HealthServerConfig};
use serde::Deserialize;

use crate::auth::{Credentials, UsernameAndApiKey};
use crate::service::Service;
use actix::{Actor, Addr};
use drogue_cloud_service_common::client::{UserAuthClient, UserAuthClientConfig};
use drogue_cloud_service_common::error::ServiceError;
use drogue_cloud_service_common::openid::TokenConfig;
use futures::TryFutureExt;
use std::sync::Arc;

use drogue_cloud_service_api::kafka::KafkaClientConfig;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,
    #[serde(default)]
    pub disable_api_keys: bool,

    #[serde(default)]
    pub health: HealthServerConfig,

    user_auth: UserAuthClientConfig,

    #[serde(default)]
    pub kafka: KafkaClientConfig,
}

#[get("/stream/{application}")]
pub async fn start_connection(
    req: HttpRequest,
    stream: Payload,
    auth: web::Either<BearerAuth, BasicAuth>,
    auth_client: web::Data<Option<Authenticator>>,
    authz_client: web::Data<Option<Arc<UserAuthClient>>>,
    authorize_api_keys: web::Data<bool>,
    application: web::Path<String>,
    service_address: web::Data<Addr<Service>>,
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
                        Err(ServiceError::InternalError(
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
    let ws = WsHandler::new(application, service_address.get_ref().clone());
    let resp = ws::start(ws, &req, stream)?;
    Ok(resp)
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    dotenv().ok();

    // Initialize config from environment variables
    let config = Config::from_env().unwrap();

    let enable_auth = config.enable_auth;

    log::info!("Starting WebSocket integration service endpoint");
    log::info!("Authentication enabled: {}", enable_auth);
    log::info!("Kafka servers: {}", config.kafka.bootstrap_servers);

    // set up security

    let (authenticator, user_auth) = if enable_auth {
        let client = reqwest::Client::new();
        let authenticator = Authenticator::new().await?;
        let user_auth = Arc::new(
            UserAuthClient::from_config(
                client,
                config.user_auth,
                TokenConfig::from_env_prefix("USER_AUTH")?.amend_with_env(),
            )
            .await?,
        );
        (Some(authenticator), Some(user_auth))
    } else {
        (None, None)
    };

    let auth = web::Data::new(authenticator);
    let authz = web::Data::new(user_auth);
    let enable_api_keys = web::Data::new(config.disable_api_keys);

    // create and start the service actor
    let service_addr = Service::default().start();
    let service_addr = web::Data::new(service_addr);

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main server

    let main = HttpServer::new(move || {
        // since we wrote our own auth service let's ignore this
        // let bearer_auth = openid_auth!(req -> {
        //     req
        //     .app_data::<web::Data<Authenticator>>()
        //     .as_ref()
        //     .map(|d| d.as_ref())
        // });

        App::new()
            .wrap(actix_web::middleware::Logger::default())
            //.wrap(Condition::new(enable_auth, bearer_auth.clone()))
            .app_data(service_addr.clone())
            .app_data(auth.clone())
            .app_data(authz.clone())
            .app_data(enable_api_keys.clone())
            .service(start_connection)
    })
    .bind(config.bind_addr)?
    .run();

    // run
    futures::try_join!(health.run(), main.err_into())?;

    // exiting
    Ok(())
}
