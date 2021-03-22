mod auth;
mod cli;
mod info;
mod spy;

use crate::auth::OpenIdClient;
use actix_cors::Cors;
use actix_web::{
    get,
    middleware::{self, Condition},
    web::{self, Data},
    App, HttpResponse, HttpServer, Responder,
};
use drogue_cloud_service_common::{
    client::{UserAuthClient, UserAuthClientConfig},
    config::ConfigFromEnv,
    defaults,
    endpoints::{create_endpoint_source, EndpointSourceType},
    openid::{create_client, Authenticator, AuthenticatorConfig},
    openid_auth,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json(json!({"success": true}))
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().finish()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::health_bind_addr")]
    pub health_bind_addr: String,
    #[serde(default = "defaults::enable_auth")]
    pub enable_auth: bool,
    #[serde(default = "defaults::kafka_bootstrap_servers")]
    pub kafka_boostrap_servers: String,
    #[serde(default = "defaults::kafka_topic")]
    pub kafka_topic: String,

    #[serde(default)]
    pub user_auth: UserAuthClientConfig,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::from_env()?;

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;

    // extract required endpoint information
    let endpoints = endpoint_source.eval_endpoints().await?;

    log::info!("Using endpoint source: {:?}", endpoint_source);
    let endpoint_source: Data<EndpointSourceType> = Data::new(endpoint_source);

    // OpenIdConnect

    let enable_auth = config.enable_auth;

    log::info!("Authentication enabled: {}", enable_auth);

    let openid_client = if enable_auth {
        let config = AuthenticatorConfig::from_env()?;
        Some(OpenIdClient {
            client: create_client(&config, endpoints.clone()).await?,
            scopes: config.scopes,
        })
    } else {
        None
    };

    let authenticator = openid_client
        .as_ref()
        .map(|client| web::Data::new(Authenticator::from_client(client.client.clone())));

    let user_auth = openid_client
        .as_ref()
        .map(|client| UserAuthClient::from_openid_client(&config.user_auth, client.client.clone()))
        .transpose()?
        .map(web::Data::new);

    let openid_client = openid_client.map(web::Data::new);

    let bind_addr = config.bind_addr.clone();

    // http server

    HttpServer::new(move || {
        let auth = openid_auth!(req -> req.app_data::<web::Data<Authenticator>>().map(|data|data.get_ref()));

        let app = App::new()
            .wrap(middleware::Logger::default())
            .wrap(Cors::permissive().supports_credentials())
            .data(web::JsonConfig::default().limit(4096))
            .data(config.clone());

        let app = if let Some(authenticator) = &authenticator {
            app.app_data(authenticator.clone())
        } else {
            app
        };

        let app = if let Some(openid_client) = &openid_client {
            app.app_data(openid_client.clone())
        } else {
            app
        };

        let app = if let Some(user_auth) = &user_auth {
            app.app_data(user_auth.clone())
        } else {
            app
        };

        app
            .app_data(endpoint_source.clone())
            .service(
                web::scope("/api/v1")
                    .wrap(Condition::new(enable_auth, auth))
                    .service(info::get_info),
            )
            // everything from here on is unauthenticated or not using the middleware
            .service(spy::stream_events) // this one is special, SSE doesn't support authorization headers
            .service(index)
            .service(cli::login)
            .service(auth::login)
            .service(auth::logout)
            .service(auth::code)
            .service(auth::refresh)
            //fixme : use a different port
            .service(health)
    })
    .bind(bind_addr)?
    .run()
    .await?;

    Ok(())
}
