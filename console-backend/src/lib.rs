mod admin;
mod api;
mod demos;
mod info;

use actix_cors::Cors;
use actix_web::{
    get, middleware,
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use anyhow::Context;
use drogue_cloud_access_token_service::{endpoints as keys, service::KeycloakAccessTokenService};
use drogue_cloud_service_api::{endpoints::Endpoints, kafka::KafkaClientConfig};
use drogue_cloud_service_common::{
    actix_auth::authentication::AuthN,
    client::{RegistryConfig, UserAuthClient, UserAuthClientConfig},
    defaults,
    health::{HealthServer, HealthServerConfig},
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig, KeycloakClient},
    openid::{AuthenticatorConfig, TokenConfig},
};
use futures::TryFutureExt;
use info::DemoFetcher;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Api;
use serde::Deserialize;

#[derive(Clone)]
pub struct OpenIdClient {
    pub client: openid::Client,
    pub scopes: String,
    pub account_url: Option<String>,
}

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

    #[serde(default = "defaults::enable_access_token")]
    pub enable_access_token: bool,

    #[serde(default)]
    pub user_auth: Option<UserAuthClientConfig>,

    pub registry: RegistryConfig,

    #[serde(default)]
    pub disable_account_url: bool,

    pub oauth: AuthenticatorConfig,

    #[serde(default = "defaults::enable_kube")]
    pub enable_kube: bool,

    #[serde(default)]
    pub workers: Option<usize>,
}

pub async fn run(config: Config, endpoints: Endpoints) -> anyhow::Result<()> {
    log::info!("Running console server!");

    log::debug!("Config: {:#?}", config);

    // kube

    let config_maps = if config.enable_kube {
        let kube = kube::client::Client::try_default()
            .await
            .context("Failed to create Kubernetes client")?;
        DemoFetcher::Kube(Api::<ConfigMap>::default_namespaced(kube))
    } else {
        DemoFetcher::None
    };

    // OpenIdConnect

    let app_config = config.clone();
    let enable_access_token = config.enable_access_token;

    let authenticator = config.oauth.into_client().await?;

    let (openid_client, user_auth) = if let Some(user_auth) = config.user_auth {
        let console_token_config = config
            .console_token_config
            .context("unable to find console token config")?;
        let ui_client = console_token_config
            .into_client(endpoints.redirect_url.clone())
            .await?;

        let user_auth = UserAuthClient::from_config(user_auth).await?;

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
            Some(user_auth),
        )
    } else {
        (None, None)
    };

    let openid_client = openid_client.map(web::Data::new);

    let bind_addr = config.bind_addr.clone();

    let keycloak_admin_client = KeycloakAdminClient::new(config.keycloak)?;
    let keycloak_service = web::Data::new(keys::WebData {
        service: KeycloakAccessTokenService {
            client: keycloak_admin_client,
        },
    });

    let registry = config.registry.into_client().await?;

    // upstream API url
    #[cfg(feature = "forward")]
    let forward_url = std::env::var("UPSTREAM_API_URL")
        .ok()
        .and_then(|url| url::Url::parse(&url).ok())
        .expect("Missing 'UPSTREAM_API_URL");

    // main server

    #[allow(clippy::let_and_return)]
    let main =
        HttpServer::new(move || {
            let auth = AuthN {
                openid: authenticator.as_ref().cloned(),
                token: user_auth.clone(),
                enable_access_token,
            };

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

            app.app_data(web::Data::new(endpoints.clone()))
                .service(
                    web::scope("/api/tokens/v1alpha1")
                        .wrap(auth.clone())
                        .service(
                            web::resource("")
                                .route(web::post().to(keys::create::<
                                    KeycloakAccessTokenService<KeycloakAdminClient>,
                                >))
                                .route(web::get().to(keys::list::<
                                    KeycloakAccessTokenService<KeycloakAdminClient>,
                                >)),
                        )
                        .service(web::resource("/{prefix}").route(
                            web::delete().to(keys::delete::<
                                KeycloakAccessTokenService<KeycloakAdminClient>,
                            >),
                        )),
                )
                .service(
                    web::scope("/api/admin/v1alpha1")
                        .wrap(auth.clone())
                        .service(web::resource("/user/whoami").route(web::get().to(admin::whoami))),
                )
                // everything from here on is unauthenticated or not using the middleware
                .service(
                    web::scope("/api/console/v1alpha1").service(
                        web::resource("/info")
                            .wrap(auth)
                            .route(web::get().to(info::get_info)),
                    ),
                )
                .service(index)
                .service(
                    web::scope("/.well-known")
                        .service(info::get_public_endpoints)
                        .service(info::get_drogue_version),
                )
        })
        .bind(bind_addr)?;

    let main = if let Some(workers) = config.workers {
        main.workers(workers).run()
    } else {
        main.run()
    };

    // run

    if let Some(health) = config.health {
        let health =
            HealthServer::new(health, vec![], Some(prometheus::default_registry().clone()));
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    Ok(())
}
