pub mod endpoints;
pub mod service;

use actix_web::{web, App, HttpServer};
use drogue_cloud_access_token_service::{
    endpoints::WebData as KeycloakWebData, service::KeycloakAccessTokenService,
};
use drogue_cloud_service_api::webapp as actix_web;
use drogue_cloud_service_common::{
    defaults,
    health::{HealthServer, HealthServerConfig},
    keycloak::client::KeycloakAdminClient,
    keycloak::KeycloakAdminClientConfig,
    keycloak::KeycloakClient,
    openid::{Authenticator, AuthenticatorConfig},
    openid_auth,
};
use futures::TryFutureExt;
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
    #[serde(default = "defaults::bind_addr")]
    pub bind_addr: String,
    #[serde(default = "defaults::max_json_payload_size")]
    pub max_json_payload_size: usize,

    pub oauth: AuthenticatorConfig,

    pub keycloak: KeycloakAdminClientConfig,

    #[serde(default)]
    pub health: Option<HealthServerConfig>,

    #[serde(default)]
    pub workers: Option<usize>,
}

#[macro_export]
macro_rules! app {
    ($data:expr, $api_key_ty:ty, $api_key:expr, $max_json_payload_size:expr, $enable_auth: expr, $auth: expr) => {
        App::new()
            .wrap(drogue_cloud_service_api::webapp::opentelemetry::RequestTracing::new())
            .wrap(actix_web::middleware::Logger::default())
            .app_data(web::JsonConfig::default().limit($max_json_payload_size))
            .app_data($data.clone())
            .app_data($api_key.clone())
            .service(
                web::scope("/api")
                    .wrap(actix_web::middleware::Condition::new($enable_auth, $auth))
                    .service(web::scope("/v1/user").service(endpoints::authorize))
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
    let max_json_payload_size = config.max_json_payload_size;

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

    let main = HttpServer::new(move || {
        let auth = openid_auth!(req -> {
            req
            .app_data::<web::Data<WebData<service::PostgresAuthorizationService>>>()
            .as_ref()
            .and_then(|data|data.authenticator.as_ref())
        });
        app!(
            data,
            KeycloakAccessTokenService<K>,
            api_key,
            max_json_payload_size,
            enable_auth,
            auth
        )
    })
    .bind(config.bind_addr)?;

    // run
    let main = if let Some(workers) = config.workers {
        main.workers(workers).run()
    } else {
        main.run()
    };

    if let Some(health) = config.health {
        let health = HealthServer::new(
            health,
            vec![Box::new(data_service)],
            Some(prometheus::default_registry().clone()),
        );
        futures::try_join!(health.run(), main.err_into())?;
    } else {
        futures::try_join!(main)?;
    }

    // exiting

    Ok(())
}
