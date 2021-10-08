mod admin;
mod api;
mod auth;
mod demos;
mod info;

#[cfg(feature = "forward")]
mod forward;

use crate::auth::OpenIdClient;
use actix_cors::Cors;
use actix_web::{
    get, middleware,
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use anyhow::Context;
use drogue_cloud_api_key_service::{
    endpoints as keys,
    service::{KeycloakApiKeyService, KeycloakApiKeyServiceConfig},
};
use drogue_cloud_service_api::{endpoints::Endpoints, kafka::KafkaClientConfig};
use drogue_cloud_service_common::{
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    defaults,
    health::{HealthServer, HealthServerConfig},
    openid::{Authenticator, TokenConfig},
    openid_auth,
};
use futures::TryFutureExt;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Api;
use serde::Deserialize;

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

    pub kafka: KafkaClientConfig,

    pub keycloak: KeycloakApiKeyServiceConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    pub console_token_config: TokenConfig,

    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    #[serde(default)]
    pub registry: Option<RegistryConfig>,

    #[serde(default)]
    pub disable_account_url: bool,
}

pub async fn run(config: Config, endpoints: Endpoints) -> anyhow::Result<()> {
    log::info!("Running console server!");
    // kube

    let kube = kube::client::Client::try_default()
        .await
        .context("Failed to create Kubernetes client")?;

    let config_maps = Api::<ConfigMap>::default_namespaced(kube.clone());

    // OpenIdConnect

    let app_config = config.clone();

    let (openid_client, user_auth, authenticator) = if let Some(user_auth) = config.user_auth {
        let client = reqwest::Client::new();
        let ui_client = config
            .console_token_config
            .into_client(client.clone(), endpoints.redirect_url.clone())
            .await?;

        let user_auth = UserAuthClient::from_config(client, user_auth).await?;

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

    let client = reqwest::Client::new();
    let registry = config
        .registry
        .context("no registry configured")?
        .into_client(client.clone())
        .await?;

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
            .app_data(web::Data::new(app_config.clone()));

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
        let app = app.app_data(web::Data::new(registry.clone()));
        let app = app.app_data(web::Data::new(config_maps.clone()));

        let app = app.app_data(web::Data::new(endpoints.clone()))
            .service(
                web::scope("/api/keys/v1alpha1")
                    .wrap(auth.clone())
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
                    .wrap(auth.clone())
                    .service(web::resource("/user/whoami").route(web::get().to(admin::whoami))),
            )
            // everything from here on is unauthenticated or not using the middleware
            .service(
                web::scope("/api/console/v1alpha1")
                    .service(
                        web::resource("/info")
                            .wrap(auth.clone())
                            .route(web::get().to(info::get_info)),
                    )
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
            .app_data(web::Data::new(awc::Client::new()))
            .app_data(web::Data::new(forward::ForwardUrl(forward_url.clone())))
            .default_service(web::route().to(forward::forward));

        app

    })
    .bind(bind_addr)?
    .run();

    // run

    if let Some(health) = config.health {
        let health = HealthServer::new(health, vec![]);
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    Ok(())
}
