mod api;
mod demos;
mod info;

use actix_web::{
    get,
    web::{self},
    HttpRequest, HttpResponse, Responder,
};
use anyhow::Context;
use drogue_client::{registry, user};
use drogue_cloud_access_token_service::{endpoints as keys, service::KeycloakAccessTokenService};
use drogue_cloud_service_api::{
    endpoints::Endpoints, health::HealthChecked, kafka::KafkaClientConfig,
    webapp::web::ServiceConfig,
};
use drogue_cloud_service_common::{
    actix::http::{CorsConfig, HttpBuilder, HttpConfig},
    actix_auth::authentication::AuthN,
    app::{Startup, StartupExt},
    auth::{
        openid::{AuthenticatorConfig, TokenConfig},
        pat,
    },
    client::ClientConfig,
    defaults,
    endpoints::create_endpoint_source,
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig, KeycloakClient},
};
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
    pub kafka: KafkaClientConfig,

    pub keycloak: KeycloakAdminClientConfig,

    /// External OpenID configuration, required to discover external OpenID endpoints
    #[serde(rename = "ui", default)]
    pub console_token_config: Option<TokenConfig>,

    #[serde(default = "defaults::oauth2_scopes")]
    pub scopes: String,

    #[serde(default)]
    pub user_auth: Option<ClientConfig>,

    pub registry: ClientConfig,

    #[serde(default)]
    pub disable_account_url: bool,

    pub oauth: AuthenticatorConfig,

    #[serde(default = "defaults::enable_kube")]
    pub enable_kube: bool,

    #[serde(default)]
    pub http: HttpConfig,
}

pub async fn configurator(
    config: Config,
    endpoints: Endpoints,
) -> Result<
    (
        impl Fn(&mut ServiceConfig) + Send + Sync + Clone + 'static,
        Vec<Box<dyn HealthChecked>>,
    ),
    anyhow::Error,
> {
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

    let authenticator = config
        .oauth
        .into_client()
        .await
        .context("Creating authenticator")?;

    let (openid_client, user_auth) = if let Some(user_auth) = config.user_auth {
        let console_token_config = config
            .console_token_config
            .context("unable to find console token config")?;
        let ui_client = console_token_config
            .into_client(endpoints.redirect_url.clone())
            .await
            .context("Creating UI client")?;

        let user_auth: user::v1::Client = user_auth.into_client().await?;

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

        log::info!("Account URL: {:?}", account_url);

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

    let keycloak_admin_client =
        KeycloakAdminClient::new(config.keycloak).context("Creating keycloak admin client")?;
    let keycloak_service = web::Data::new(keys::WebData {
        service: KeycloakAccessTokenService {
            client: keycloak_admin_client,
        },
    });

    let registry: registry::v1::Client = config
        .registry
        .into_client()
        .await
        .context("Creating registry client")?;

    Ok((
        move |cfg: &mut ServiceConfig| {
            let auth = AuthN::from((
                authenticator.clone(),
                user_auth.clone().map(pat::Authenticator::new),
            ));

            let app = cfg
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
                // everything from here on is unauthenticated or not using the middleware
                .service(index)
                .service(
                    web::scope("/.well-known")
                        .service(info::get_public_endpoints)
                        .service(info::get_drogue_version),
                );
        },
        vec![],
    ))
}

pub async fn run(config: Config, startup: &mut dyn Startup) -> anyhow::Result<()> {
    log::info!("Running console server!");
    log::debug!("Config: {:#?}", config);

    // the endpoint source we choose
    let endpoint_source = create_endpoint_source()?;
    log::info!("Using endpoint source: {:#?}", endpoint_source);
    let endpoints = endpoint_source.eval_endpoints().await?;

    // main server

    let (cfg, checks) = configurator(config.clone(), endpoints).await?;
    HttpBuilder::new(config.http.clone(), Some(startup.runtime_config()), cfg)
        .default_cors(CorsConfig::permissive())
        .start(startup)?;
    // spawn

    startup.check_iter(checks);

    // done

    Ok(())
}
