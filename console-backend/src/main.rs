mod api;
mod auth;
mod info;
mod spy;

use crate::auth::OpenIdClient;
use actix_cors::Cors;
use actix_web::{
    get,
    middleware::{self, Condition},
    web::{self, Data},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use drogue_cloud_api_key_service::{
    endpoints as keys,
    service::{KeycloakApiKeyService, KeycloakApiKeyServiceConfig},
};
use drogue_cloud_service_common::{
    client::{UserAuthClient, UserAuthClientConfig},
    config::ConfigFromEnv,
    defaults,
    endpoints::{create_endpoint_source, EndpointSourceType},
    health::{HealthServer, HealthServerConfig},
    openid::{Authenticator, TokenConfig},
    openid_auth,
};
use futures::TryFutureExt;
use serde::Deserialize;

#[get("/")]
async fn index(req: HttpRequest, client: web::Data<OpenIdClient>) -> impl Responder {
    match api::spec(req, client) {
        Ok(spec) => HttpResponse::Ok().json(spec),
        Err(err) => {
            log::warn!("Failed to generate OpenAPI spec: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
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

    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,

    #[serde(default)]
    pub user_auth: UserAuthClientConfig,

    pub keycloak: KeycloakApiKeyServiceConfig,

    #[serde(default)]
    pub health: HealthServerConfig,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::from_env()?;

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;
    let endpoints = endpoint_source.eval_endpoints().await?;

    log::info!("Using endpoint source: {:?}", endpoint_source);
    let endpoint_source: Data<EndpointSourceType> = Data::new(endpoint_source);

    // OpenIdConnect

    let enable_auth = config.enable_auth;
    let app_config = config.clone();

    log::info!("Authentication enabled: {}", enable_auth);

    let (openid_client, user_auth, authenticator) = if enable_auth {
        let client = reqwest::Client::new();
        let ui_client = TokenConfig::from_env_prefix("UI")?
            .amend_with_env()
            .into_client(client.clone(), endpoints.redirect_url)
            .await?;

        let user_auth = UserAuthClient::from_config(
            client,
            config.user_auth,
            TokenConfig::from_env_prefix("USER_AUTH")?.amend_with_env(),
        )
        .await?;

        (
            Some(OpenIdClient {
                client: ui_client,
                scopes: config.scopes.clone(),
            }),
            Some(web::Data::new(user_auth)),
            Some(web::Data::new(Authenticator::new().await?)),
        )
    } else {
        (None, None, None)
    };

    let openid_client = openid_client.map(web::Data::new);

    let bind_addr = config.bind_addr.clone();

    let keycloak_service = web::Data::new(keys::WebData {
        service: KeycloakApiKeyService::new(config.keycloak)?,
    });

    // health server

    let health = HealthServer::new(config.health, vec![]);

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> req.app_data::<web::Data<Authenticator>>().map(|data|data.get_ref()));

        let app = App::new()
            .wrap(middleware::Logger::default())
            .wrap(Cors::permissive())
            .data(web::JsonConfig::default().limit(4096))
            .data(app_config.clone());

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

        let app = app.app_data(keycloak_service.clone());

        app
            .app_data(endpoint_source.clone())
            .service(
                web::scope("/api/v1")
                    .wrap(Condition::new(enable_auth, auth.clone()))
                    .service(info::get_info),
            )
            .service(
                web::scope("/api/keys/v1alpha1")
                    .wrap(Condition::new(enable_auth, auth))
                    .service(
                        web::resource("")
                            .route(web::post().to(keys::create::<KeycloakApiKeyService>))
                            .route(web::get().to(keys::list::<KeycloakApiKeyService>))
                    )
                    .service(
                        web::resource("/{prefix}")
                        .route(web::delete().to(keys::delete::<KeycloakApiKeyService>))
                    )
            )
            // everything from here on is unauthenticated or not using the middleware
            .service(spy::stream_events) // this one is special, SSE doesn't support authorization headers
            .service(index)
            .service(auth::login)
            .service(auth::logout)
            .service(auth::code)
            .service(auth::refresh)
            .service(
                web::scope("/.well-known")
                    .service(info::get_public_endpoints)
                    .service(info::get_drogue_version)
            )

    })
    .bind(bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
