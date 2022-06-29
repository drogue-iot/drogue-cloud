pub mod endpoints;
pub mod service;

use actix_web::web;
use drogue_cloud_access_token_service::{
    endpoints::WebData as KeycloakWebData, service::KeycloakAccessTokenService,
};
use drogue_cloud_service_api::{health::BoxedHealthChecked, webapp as actix_web};
use drogue_cloud_service_common::{
    actix::{HttpBuilder, HttpConfig},
    app::run_main,
    health::HealthServerConfig,
    keycloak::{client::KeycloakAdminClient, KeycloakAdminClientConfig, KeycloakClient},
    openid::{Authenticator, AuthenticatorConfig},
    openid_auth,
};
use serde::Deserialize;
use service::AuthorizationServiceConfig;

pub struct WebData<S>
where
    S: service::AuthorizationService,
{
    pub service: S,
    pub authenticator: Option<Authenticator>,
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub service: AuthorizationServiceConfig,

    pub oauth: AuthenticatorConfig,

    pub keycloak: KeycloakAdminClientConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub http: HttpConfig,
}

#[macro_export]
macro_rules! app {
    ($cfg:expr, $data:expr, $api_key_ty:ty, $api_key:expr, $enable_auth:expr, $auth:expr) => {
        $cfg.app_data($data.clone())
            .app_data($api_key.clone())
            .service(
                web::scope("/api")
                    .wrap(actix_web::middleware::Condition::new($enable_auth, $auth))
                    .service(web::scope("/v1/user").service(
                        web::resource("/authz").route(web::post().to(endpoints::authorize)),
                    ))
                    .service(web::resource("/user/v1alpha1/authn").route(web::post().to(
                        drogue_cloud_access_token_service::endpoints::authenticate::<$api_key_ty>,
                    ))),
            )
    };
}

pub async fn run<K>(config: Config) -> anyhow::Result<()>
where
    K: 'static + KeycloakClient + std::marker::Send + std::marker::Sync,
{
    let authenticator = config.oauth.into_client().await?;
    let enable_auth = authenticator.is_some();

    let data = web::Data::new(WebData {
        authenticator,
        service: service::PostgresAuthorizationService::new(config.service)?,
    });

    let keycloak_client = KeycloakAdminClient::new(config.keycloak)?;
    let api_key = web::Data::new(KeycloakWebData {
        service: KeycloakAccessTokenService {
            client: keycloak_client,
        },
    });

    let data_service = data.service.clone();

    // main server

    let main = HttpBuilder::new(config.http, move |cfg| {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthorizationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        app!(
            cfg,
            data,
            KeycloakAccessTokenService<K>,
            api_key,
            enable_auth,
            auth
        );
    })
    .run()?;

    run_main([main], config.health, [data_service.boxed()]).await?;

    // exiting

    Ok(())
}
