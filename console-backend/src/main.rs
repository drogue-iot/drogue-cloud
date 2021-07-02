mod admin;
mod api;
mod auth;
mod info;
mod spy;

#[cfg(feature = "forward")]
mod forward;

use crate::auth::OpenIdClient;
use actix_cors::Cors;
use actix_web::{
    get,
    middleware::{self, Condition},
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use drogue_cloud_api_key_service::{
    endpoints as keys,
    service::{KeycloakApiKeyService, KeycloakApiKeyServiceConfig},
};
use drogue_cloud_service_api::endpoints::Endpoints;
use drogue_cloud_service_common::{
    client::{UserAuthClient, UserAuthClientConfig},
    config::ConfigFromEnv,
    defaults,
    endpoints::create_endpoint_source,
    health::{HealthServer, HealthServerConfig},
    openid::{Authenticator, TokenConfig},
    openid_auth,
};
use futures::TryFutureExt;
use serde::Deserialize;
use std::collections::HashMap;

#[get("/")]
async fn index(
    req: HttpRequest,
    endpoints: web::Data<Endpoints>,
    client: web::Data<OpenIdClient>,
) -> impl Responder {
    match api::spec(req, endpoints.get_ref(), client) {
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
    pub kafka_bootstrap_servers: String,
    #[serde(default = "defaults::kafka_events_topic")]
    pub kafka_topic: String,
    #[serde(default)]
    pub kafka_properties: HashMap<String, String>,

    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,

    #[serde(default)]
    pub user_auth: UserAuthClientConfig,

    pub keycloak: KeycloakApiKeyServiceConfig,

    #[serde(default)]
    pub health: HealthServerConfig,

    #[serde(default)]
    pub disable_account_url: bool,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = Config::from_env()?;

    // the endpoint source we choose

    let endpoint_source = create_endpoint_source()?;
    log::info!("Using endpoint source: {:?}", endpoint_source);
    let endpoints = endpoint_source.eval_endpoints().await?;

    // OpenIdConnect

    let enable_auth = config.enable_auth;
    let app_config = config.clone();

    log::info!("Authentication enabled: {}", enable_auth);

    let (openid_client, user_auth, authenticator) = if enable_auth {
        let client = reqwest::Client::new();
        let ui_client = TokenConfig::from_env_prefix("UI")?
            .amend_with_env()
            .into_client(client.clone(), endpoints.redirect_url.clone())
            .await?;

        let user_auth = UserAuthClient::from_config(
            client,
            config.user_auth,
            TokenConfig::from_env_prefix("USER_AUTH")?.amend_with_env(),
        )
        .await?;

        let account_url = match config.disable_account_url {
            true => None,
            false => Some({
                // this only works with Keycloak, but you can deactivate it
                let mut issuer = ui_client.config().issuer.clone();
                issuer
                    .path_segments_mut()
                    .map_err(|_| anyhow::anyhow!("Failed to modify path"))?
                    .push("account");
                issuer.into()
            }),
        };

        log::debug!("Account URL: {:?}", account_url);

        (
            Some(OpenIdClient {
                client: ui_client,
                scopes: config.scopes.clone(),
                account_url,
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

    // upstream API url
    #[cfg(feature = "forward")]
    let forward_url = std::env::var("UPSTREAM_API_URL")
        .ok()
        .and_then(|url| url::Url::parse(&url).ok())
        .expect("Missing 'UPSTREAM_API_URL");

    // main server

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> req.app_data::<web::Data<Authenticator>>().map(|data|data.get_ref()));

        let app = App::new()
            .wrap(Cors::permissive())
            .wrap(middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit(4096))
            .app_data(app_config.clone());

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

        let app = app.app_data(endpoints.clone())
            .service(
                web::scope("/api/keys/v1alpha1")
                    .wrap(Condition::new(enable_auth, auth.clone()))
                    .service(
                        web::resource("")
                            .route(web::post().to(keys::create::<KeycloakApiKeyService>))
                            .route(web::get().to(keys::list::<KeycloakApiKeyService>)),
                    )
                    .service(
                        web::resource("/{prefix}")
                            .route(web::delete().to(keys::delete::<KeycloakApiKeyService>)),
                    ),
            )
            .service(
                web::scope("/api/admin/v1alpha1")
                    .wrap(Condition::new(enable_auth, auth.clone()))
                    .service(web::resource("/user/whoami").route(web::get().to(admin::whoami))),
            )
            // everything from here on is unauthenticated or not using the middleware
            .service(
                web::scope("/api/console/v1alpha1")
                    .service(
                        web::resource("/info")
                            .wrap(Condition::new(enable_auth, auth.clone()))
                            .route(web::get().to(info::get_info)),
                    )
                    .service(spy::stream_events) // this one is special, SSE doesn't support authorization headers
                    .service(auth::login)
                    .service(auth::logout)
                    .service(auth::code)
                    .service(auth::refresh),
            )
            .service(index)
            .service(
                web::scope("/.well-known")
                    .service(info::get_public_endpoints)
                    .service(info::get_drogue_version),
            );

        #[cfg(feature = "forward")]
        let app = app
            .data(awc::Client::new())
            .data(forward_url.clone())
            .default_service(web::route().to(forward::forward));

        app

    })
    .bind(bind_addr)?
    .run();

    // run

    futures::try_join!(health.run(), main.err_into())?;

    // exiting

    Ok(())
}
