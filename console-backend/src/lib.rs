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
use drogue_cloud_api_key_service::{endpoints as keys, service::KeycloakApiKeyService};
use drogue_cloud_service_api::{endpoints::Endpoints, kafka::KafkaClientConfig};
use drogue_cloud_service_common::{
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    defaults,
    health::{HealthServer, HealthServerConfig},
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig, KeycloakClient},
    openid::{Authenticator, AuthenticatorConfig, TokenConfig},
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

    pub kafka: KafkaClientConfig,

    pub keycloak: KeycloakAdminClientConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(rename = "ui", default)]
    pub console_token_config: Option<TokenConfig>,

    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub disable_account_url: bool,

    pub oauth: AuthenticatorConfig,

    #[serde(default = "defaults::enable_kube")]
    pub enable_kube: bool,
}

pub async fn run(config: Config, endpoints: Endpoints) -> anyhow::Result<()> {
    log::info!("Running console server!");

    log::debug!("Config: {:#?}", config);

    // kube

    let config_maps = if config.enable_kube {
        let kube = kube::client::Client::try_default()
            .await
            .context("Failed to create Kubernetes client")?;
        Some(Api::<ConfigMap>::default_namespaced(kube.clone()))
    } else {
        None
    };

    let client = reqwest::Client::new();

    // OpenIdConnect

    let app_config = config.clone();

    let authenticator = config.oauth.into_client().await?.map(web::Data::new);

    let (openid_client, user_auth) = if let Some(user_auth) = config.user_auth {
        let console_token_config = config
            .console_token_config
            .context("unable to find console token config")?;
        let ui_client = console_token_config
            .into_client(client.clone(), endpoints.redirect_url.clone())
            .await?;

        let user_auth = UserAuthClient::from_config(client.clone(), user_auth).await?;

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
        )
    } else {
        (None, None)
    };

    let openid_client = openid_client.map(web::Data::new);

    let bind_addr = config.bind_addr.clone();

    let keycloak_admin_client = KeycloakAdminClient::new(config.keycloak)?;
    let keycloak_service = web::Data::new(keys::WebData {
        service: KeycloakApiKeyService {
            client: keycloak_admin_client,
        },
    });

    let registry = config.registry.into_client(client.clone()).await?;

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
        let app = if let Some(config_maps) = config_maps.clone() {
            app.app_data(web::Data::new(config_maps))
        } else {
            app
        };

        let app = app.app_data(web::Data::new(endpoints.clone()))
            .service(
                web::scope("/api/keys/v1alpha1")
                    .wrap(auth.clone())
                    .service(
                        web::resource("")
                            .route(web::post().to(keys::create::<KeycloakApiKeyService<KeycloakAdminClient>>))
                            .route(web::get().to(keys::list::<KeycloakApiKeyService<KeycloakAdminClient>>)),
                    )
                    .service(
                        web::resource("/{prefix}")
                            .route(web::delete().to(keys::delete::<KeycloakApiKeyService<KeycloakAdminClient>>)),
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
